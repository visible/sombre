use std::collections::VecDeque;
use std::io::{self, ErrorKind};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

const WAIT: Duration = Duration::from_secs(25);
const TICK: Duration = Duration::from_millis(100);
const PATIENCE: Duration = Duration::from_secs(10);
const PING: Duration = Duration::from_secs(25);
const SILENCE: Duration = Duration::from_secs(60);
const FRAME: usize = 16 * 1024 * 1024;

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

#[derive(Default)]
struct State {
    out: VecDeque<String>,
    inbox: VecDeque<String>,
    devices: Option<String>,
}

#[derive(Default)]
pub struct Wire {
    state: Mutex<State>,
    changed: Condvar,
    stop: AtomicBool,
}

pub fn address(base: &str, room: &str, secret: &str) -> String {
    let scheme = if base.starts_with("https://") {
        "wss"
    } else {
        "ws"
    };
    let host = base.split_once("://").map_or(base, |(_, host)| host);
    format!("{scheme}://{host}/r/{room}/in?secret={secret}")
}

impl Wire {
    pub fn open(url: String) -> Arc<Self> {
        let wire = Arc::new(Self::default());
        let running = Arc::clone(&wire);
        thread::spawn(move || running.run(&url));
        wire
    }

    pub fn push(&self, item: String) {
        if !self.stopped()
            && let Ok(mut state) = self.state.lock()
        {
            state.out.push_back(item);
        }
    }

    pub fn next(&self) -> Option<String> {
        let state = self.state.lock().ok()?;
        let (mut state, _) = self
            .changed
            .wait_timeout_while(state, WAIT, |state| {
                state.inbox.is_empty() && !self.stopped()
            })
            .ok()?;
        if self.stopped() {
            return None;
        }
        Some(state.inbox.pop_front().unwrap_or_else(|| "{}".to_string()))
    }

    pub fn devices(&self) -> Option<String> {
        self.state.lock().ok()?.devices.clone()
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.changed.notify_all();
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn run(&self, url: &str) {
        let mut fresh = true;
        let mut after: Option<u64> = None;
        let mut tries: u32 = 0;
        while !self.stopped() {
            let target =
                after.map_or_else(|| url.to_string(), |after| format!("{url}&after={after}"));
            if let Ok(mut socket) = dial(&target) {
                tries = 0;
                if self.talk(&mut socket, &mut fresh, &mut after) {
                    self.stop();
                    return;
                }
            }
            tries = tries.saturating_add(1);
            thread::sleep(
                Duration::from_millis(500 * 2_u64.pow(tries.min(4))).min(Duration::from_secs(5)),
            );
        }
    }

    fn talk(&self, socket: &mut Socket, fresh: &mut bool, after: &mut Option<u64>) -> bool {
        let mut heard = Instant::now();
        let mut pinged = Instant::now();
        loop {
            if self.stopped() {
                let _ = socket.close(None);
                return true;
            }
            let batch = self.take();
            if *fresh || !batch.is_empty() {
                let mut frame = json!({"out": batch});
                if *fresh {
                    frame["fresh"] = json!(true);
                }
                if socket.send(Message::text(frame.to_string())).is_err() {
                    self.restore(frame);
                    return false;
                }
                *fresh = false;
            }
            if pinged.elapsed() >= PING {
                if socket.send(Message::text("ping")).is_err() {
                    return false;
                }
                pinged = Instant::now();
            }
            match socket.read() {
                Ok(Message::Text(text)) => {
                    heard = Instant::now();
                    if let Some(next) = self.hear(&text) {
                        *after = Some(next);
                        if socket
                            .send(Message::text(json!({"read": next}).to_string()))
                            .is_err()
                        {
                            return false;
                        }
                    }
                }
                Ok(Message::Close(frame)) => {
                    return frame.is_some_and(|frame| matches!(u16::from(frame.code), 4403 | 4404));
                }
                Ok(_) => heard = Instant::now(),
                Err(tungstenite::Error::Io(error))
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => return false,
            }
            if heard.elapsed() > SILENCE {
                return false;
            }
        }
    }

    fn take(&self) -> Vec<String> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        let mut size = 0;
        let mut batch = Vec::new();
        while let Some(item) = state.out.front() {
            if !batch.is_empty() && size + item.len() > FRAME {
                break;
            }
            size += item.len();
            batch.extend(state.out.pop_front());
        }
        batch
    }

    fn restore(&self, frame: Value) {
        if let Ok(mut state) = self.state.lock() {
            for item in frame["out"].as_array().into_iter().flatten().rev() {
                if let Some(item) = item.as_str() {
                    state.out.push_front(item.to_string());
                }
            }
        }
    }

    fn hear(&self, text: &str) -> Option<u64> {
        let frame: Value = serde_json::from_str(text).ok()?;
        let mut state = self.state.lock().ok()?;
        if let Some(devices) = frame.get("devices") {
            state.devices = Some(devices.to_string());
        }
        let next = frame["next"].as_u64()?;
        state.inbox.push_back(text.to_string());
        drop(state);
        self.changed.notify_all();
        Some(next)
    }
}

fn dial(target: &str) -> io::Result<Socket> {
    let uri: tungstenite::http::Uri = target.parse().map_err(io::Error::other)?;
    let host = uri.host().ok_or_else(|| io::Error::other("no host"))?;
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("wss") {
            443
        } else {
            80
        });
    let place = (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::other("no address"))?;
    let stream = TcpStream::connect_timeout(&place, PATIENCE)?;
    stream.set_read_timeout(Some(PATIENCE))?;
    stream.set_write_timeout(Some(PATIENCE))?;
    let (socket, _) = tungstenite::client_tls(target, stream)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let tcp = match socket.get_ref() {
        MaybeTlsStream::Plain(tcp) => tcp,
        MaybeTlsStream::Rustls(tls) => tls.get_ref(),
        _ => return Err(io::Error::other("unknown stream")),
    };
    tcp.set_read_timeout(Some(TICK))?;
    Ok(socket)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_addresses_follow_the_relay_scheme() {
        assert_eq!(
            address("https://relay.example.com", "r1", "s1"),
            "wss://relay.example.com/r/r1/in?secret=s1"
        );
        assert_eq!(
            address("http://localhost:8787", "r1", "s1"),
            "ws://localhost:8787/r/r1/in?secret=s1"
        );
    }

    #[test]
    fn frames_fill_the_inbox_and_cache_phones() {
        let wire = Wire::default();
        assert_eq!(wire.hear(r#"{"devices":[{"id":"d1"}]}"#), None);
        assert_eq!(wire.devices().as_deref(), Some(r#"[{"id":"d1"}]"#));
        assert_eq!(wire.hear(r#"{"items":["a"],"next":4}"#), Some(4));
        assert_eq!(wire.next().as_deref(), Some(r#"{"items":["a"],"next":4}"#));
        assert_eq!(wire.hear("nope"), None);
    }

    #[test]
    fn failed_sends_go_back_to_the_front_in_order() {
        let wire = Wire::default();
        wire.push("a".to_string());
        wire.push("b".to_string());
        let batch = wire.take();
        assert_eq!(batch, ["a", "b"]);
        wire.push("c".to_string());
        wire.restore(json!({"out": batch}));
        assert_eq!(wire.take(), ["a", "b", "c"]);
        wire.stop();
        wire.push("d".to_string());
        assert!(wire.take().is_empty());
        assert_eq!(wire.next(), None);
    }
}
