use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};

use crate::tailscale::Node;
use crate::wire::Wire;

const PAGE: &str = include_str!("phone.html");
const SCRIPT: &str = include_str!("phone.js");
const BODY: usize = 6 * 1024 * 1024;
const PICTURE: u64 = 4 * 1024 * 1024;
const KEPT: usize = 4_000;
const ROOMS: usize = 500;
const WAIT: Duration = Duration::from_secs(25);
const SEEN: usize = 64;
const PHOTOS: usize = 4;
const HOSTED: &str = "https://sombre.si";

type Rooms = HashMap<String, Room>;

#[derive(Debug, PartialEq, Eq)]
pub enum Inbound {
    Message {
        text: String,
        now: bool,
        images: Vec<Vec<u8>>,
    },
    Answer {
        id: String,
        yes: bool,
    },
    Reply {
        id: String,
        answers: Option<Vec<String>>,
    },
    Outdated,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Arrival {
    pub text: String,
    pub now: bool,
    pub images: Vec<std::path::PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub label: String,
    pub since: u64,
}

#[derive(Default)]
struct Room {
    epoch: String,
    secret: String,
    tokens: Vec<String>,
    devices: Vec<Device>,
    out: VecDeque<String>,
    outbase: usize,
    inbox: VecDeque<String>,
    inbase: usize,
    seen: VecDeque<String>,
}

pub struct Hub {
    rooms: Mutex<Rooms>,
    changed: Condvar,
    stop: AtomicBool,
    open: bool,
    password: Option<String>,
}

fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    let _ = SystemRandom::new().fill(&mut bytes);
    bytes
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn seal(key: &[u8; 32], plain: &[u8]) -> Option<String> {
    let sealing = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, key).ok()?);
    let nonce: [u8; 12] = random();
    let mut buffer = plain.to_vec();
    sealing
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::empty(),
            &mut buffer,
        )
        .ok()?;
    let mut framed = nonce.to_vec();
    framed.extend(buffer);
    Some(URL_SAFE_NO_PAD.encode(framed))
}

pub fn unseal(key: &[u8; 32], text: &str) -> Option<Vec<u8>> {
    let framed = URL_SAFE_NO_PAD.decode(text.trim()).ok()?;
    if framed.len() < 12 + 16 {
        return None;
    }
    let (nonce, sealed) = framed.split_at(12);
    let opening = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, key).ok()?);
    let mut buffer = sealed.to_vec();
    let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
    let plain = opening
        .open_in_place(nonce, Aad::empty(), &mut buffer)
        .ok()?;
    Some(plain.to_vec())
}

pub fn inbound(plain: &[u8]) -> Option<Inbound> {
    let message: Value = serde_json::from_slice(plain).ok()?;
    match message["kind"].as_str()? {
        "message" => {
            let text = sombre_core::scrub::scrub(message["text"].as_str().unwrap_or_default());
            let text = text.trim();
            let images: Vec<Vec<u8>> = message["images"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|image| STANDARD.decode(image["data"].as_str()?).ok())
                .filter(|bytes| sombre_harness::image::kind(bytes).is_some())
                .take(PHOTOS)
                .collect();
            (!text.is_empty() || !images.is_empty()).then(|| Inbound::Message {
                images,
                now: message["now"].as_bool().unwrap_or(false),
                text: text.to_string(),
            })
        }
        "answer" | "reply" if message["id"].as_str().is_none_or(str::is_empty) => {
            Some(Inbound::Outdated)
        }
        "answer" => message["yes"].as_bool().map(|yes| Inbound::Answer {
            id: message["id"].as_str().unwrap_or_default().to_string(),
            yes,
        }),
        "reply" => Some(Inbound::Reply {
            id: message["id"].as_str().unwrap_or_default().to_string(),
            answers: message["answers"].as_array().map(|answers| {
                answers
                    .iter()
                    .take(SEEN)
                    .map(|answer| {
                        sombre_core::scrub::scrub(answer.as_str().unwrap_or_default())
                            .trim()
                            .to_string()
                    })
                    .collect()
            }),
        }),
        _ => None,
    }
}

pub fn label(agent: &str) -> String {
    let device = [
        ("iPhone", "iPhone"),
        ("iPad", "iPad"),
        ("Android", "Android"),
        ("Macintosh", "Mac"),
        ("Windows", "Windows"),
    ]
    .iter()
    .find(|(needle, _)| agent.contains(needle))
    .map_or("browser", |(_, name)| name);
    let browser = [
        ("CriOS", "Chrome"),
        ("FxiOS", "Firefox"),
        ("Edg", "Edge"),
        ("Chrome", "Chrome"),
        ("Firefox", "Firefox"),
        ("Safari", "Safari"),
    ]
    .iter()
    .find(|(needle, _)| agent.contains(needle))
    .map(|(_, name)| *name);
    browser.map_or_else(
        || device.to_string(),
        |browser| format!("{device}, {browser}"),
    )
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub agent: String,
    pub body: String,
}

impl Request {
    fn get(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

pub fn parse(reader: &mut impl BufRead) -> Option<Request> {
    let mut first = String::new();
    reader.read_line(&mut first).ok()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut length = 0;
    let mut agent = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok()?;
            } else if name.eq_ignore_ascii_case("user-agent") {
                agent = value.trim().to_string();
            }
        }
    }
    if length > BODY {
        return None;
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        agent,
        body: String::from_utf8(body).ok()?,
        method,
        path: path.to_string(),
        query: pairs(query),
    })
}

fn pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn reply(stream: &mut TcpStream, status: &str, kind: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn since(log: &VecDeque<String>, base: usize, after: usize) -> (Vec<String>, usize) {
    let start = after.max(base) - base;
    (log.iter().skip(start).cloned().collect(), base + log.len())
}

fn push(log: &mut VecDeque<String>, base: &mut usize, item: String) {
    log.push_back(item);
    if log.len() > KEPT {
        log.pop_front();
        *base += 1;
    }
}

type Answer = (&'static str, &'static str, String);

fn ok(kind: &'static str, body: String) -> Answer {
    ("200 OK", kind, body)
}

fn refuse(text: &str) -> Answer {
    ("403 Forbidden", "text/plain", text.to_string())
}

fn missing(text: &str) -> Answer {
    ("404 Not Found", "text/plain", text.to_string())
}

impl Hub {
    pub fn new(open: bool, password: Option<String>) -> Self {
        Self {
            changed: Condvar::new(),
            open,
            password,
            rooms: Mutex::new(HashMap::new()),
            stop: AtomicBool::new(false),
        }
    }

    pub fn create(&self) -> Option<(String, String)> {
        let mut rooms = self.rooms.lock().ok()?;
        if rooms.len() >= ROOMS
            && let Some(oldest) = rooms.keys().next().cloned()
        {
            rooms.remove(&oldest);
        }
        let room = hex(&random::<12>());
        let secret = hex(&random::<24>());
        rooms.insert(
            room.clone(),
            Room {
                epoch: hex(&random::<6>()),
                secret: secret.clone(),
                ..Room::default()
            },
        );
        Some((room, secret))
    }

    fn restore(&self, room: &str, secret: &str, devices: Vec<Device>) {
        if let Ok(mut rooms) = self.rooms.lock() {
            rooms.insert(
                room.to_string(),
                Room {
                    devices,
                    epoch: hex(&random::<6>()),
                    secret: secret.to_string(),
                    ..Room::default()
                },
            );
        }
    }

    fn wait<'a>(
        &'a self,
        rooms: MutexGuard<'a, Rooms>,
        done: impl Fn(&Rooms) -> bool,
    ) -> MutexGuard<'a, Rooms> {
        self.changed
            .wait_timeout_while(rooms, WAIT, |rooms| {
                !done(rooms) && !self.stop.load(Ordering::SeqCst)
            })
            .map_or_else(|error| error.into_inner().0, |(rooms, _)| rooms)
    }

    fn handle(&self, request: &Request) -> Answer {
        if request.method == "GET" && request.path == "/app.js" {
            return ok("text/javascript; charset=utf-8", served("phone.js", SCRIPT));
        }
        if request.method == "POST" && request.path == "/rooms" {
            let allowed = self.open
                && self
                    .password
                    .as_deref()
                    .is_none_or(|password| request.get("password") == Some(password));
            if !allowed {
                return refuse("this relay doesn't take new rooms");
            }
            return self.create().map_or_else(
                || missing("couldn't make a room"),
                |(room, secret)| {
                    ok(
                        "application/json",
                        json!({"room": room, "secret": secret}).to_string(),
                    )
                },
            );
        }
        let mut parts = request.path.trim_start_matches('/').splitn(3, '/');
        let (Some("r"), Some(id)) = (parts.next(), parts.next()) else {
            return missing("not found");
        };
        let action = parts.next().unwrap_or("");
        let key = id.to_string();
        let after: usize = request
            .get("after")
            .and_then(|after| after.parse().ok())
            .unwrap_or(0);
        let Ok(mut rooms) = self.rooms.lock() else {
            return missing("the relay is shutting down");
        };
        let Some(room) = rooms.get_mut(id) else {
            return missing(
                "this chat isn't paired anymore. run /pair on your computer for a new link",
            );
        };
        let device = request
            .get("device")
            .is_some_and(|device| room.devices.iter().any(|known| known.id == device));
        let owner = request.get("secret") == Some(room.secret.as_str());
        let answer = match (request.method.as_str(), action) {
            ("GET", "") => ok("text/html; charset=utf-8", served("phone.html", PAGE)),
            ("POST", "pair") => match room
                .tokens
                .iter()
                .position(|token| token == request.body.trim())
            {
                Some(spot) => {
                    room.tokens.remove(spot);
                    let fresh = hex(&random::<16>());
                    room.devices.push(Device {
                        id: fresh.clone(),
                        label: label(&request.agent),
                        since: sombre_core::now(),
                    });
                    ok("text/plain", fresh)
                }
                None => refuse(
                    "this pairing link was already used. run /pair on your computer for a new one",
                ),
            },
            ("GET", "events") if device => {
                let rooms = self.wait(rooms, |rooms| {
                    rooms
                        .get(&key)
                        .is_none_or(|room| room.outbase + room.out.len() != after)
                });
                return rooms.get(&key).map_or_else(
                    || missing("this chat isn't paired anymore"),
                    |room| {
                        let (items, next) = since(&room.out, room.outbase, after);
                        ok(
                            "application/json",
                            json!({"epoch": room.epoch, "items": items, "next": next}).to_string(),
                        )
                    },
                );
            }
            ("GET", "in") if owner => {
                let rooms = self.wait(rooms, |rooms| {
                    rooms
                        .get(&key)
                        .is_none_or(|room| room.inbase + room.inbox.len() > after)
                });
                return rooms.get(&key).map_or_else(
                    || missing("this chat isn't paired anymore"),
                    |room| {
                        let (items, next) = since(&room.inbox, room.inbase, after);
                        ok(
                            "application/json",
                            json!({"items": items, "next": next}).to_string(),
                        )
                    },
                );
            }
            ("POST", "send") if device => {
                let Room {
                    inbox,
                    inbase,
                    seen,
                    ..
                } = room;
                if let Some(id) = request.get("id") {
                    if seen.iter().any(|known| known == id) {
                        return ok("text/plain", "ok".to_string());
                    }
                    seen.push_back(id.to_string());
                    if seen.len() > SEEN {
                        seen.pop_front();
                    }
                }
                push(inbox, inbase, request.body.trim().to_string());
                ok("text/plain", "ok".to_string())
            }
            ("POST", "out") if owner => {
                let Room { out, outbase, .. } = room;
                push(out, outbase, request.body.trim().to_string());
                ok("text/plain", "ok".to_string())
            }
            ("POST", "token") if owner => {
                room.tokens.push(request.body.trim().to_string());
                ok("text/plain", "ok".to_string())
            }
            ("GET", "devices") if owner => {
                let listed: Vec<Value> = room
                    .devices
                    .iter()
                    .map(|device| json!({"id": device.id, "label": device.label, "since": device.since}))
                    .collect();
                ok("application/json", Value::Array(listed).to_string())
            }
            ("POST", "forget") if owner => {
                room.devices
                    .retain(|device| device.id != request.body.trim());
                ok("text/plain", "ok".to_string())
            }
            ("POST", "close") if owner => {
                rooms.remove(id);
                ok("text/plain", "ok".to_string())
            }
            _ => {
                refuse("this phone isn't paired anymore. run /pair on your computer for a new link")
            }
        };
        drop(rooms);
        self.changed.notify_all();
        answer
    }

    fn serve(&self, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let Some(request) = parse(&mut BufReader::new(&mut stream)) else {
            return reply(&mut stream, "400 Bad Request", "text/plain", "bad request");
        };
        let (status, kind, body) = self.handle(&request);
        reply(&mut stream, status, kind, &body);
    }

    pub fn listen(self: &Arc<Self>, listener: &TcpListener) {
        for stream in listener.incoming() {
            if self.stop.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(stream) = stream {
                let hub = Arc::clone(self);
                thread::spawn(move || hub.serve(stream));
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    Wifi,
    Tailnet,
    Relay,
    Hosted,
}

enum Link {
    Here {
        hub: Arc<Hub>,
        room: String,
        secret: String,
        address: (IpAddr, u16),
        bind: IpAddr,
        reach: Reach,
        forwarded: Option<PathBuf>,
    },
    There(Remote),
    Cloud(Remote, Arc<Wire>),
}

struct Remote {
    base: String,
    room: String,
    secret: String,
}

impl Remote {
    fn call(&self, method: &str, action: &str, query: &str, body: &str) -> Option<String> {
        let Self { base, room, secret } = self;
        let url = format!("{base}/r/{room}/{action}?secret={secret}&{query}");
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(40)))
            .build()
            .into();
        let response = if method == "POST" {
            agent.post(&url).send(body)
        } else {
            agent.get(&url).call()
        };
        response.ok()?.body_mut().read_to_string().ok()
    }

    fn page(&self) -> String {
        format!("{}/r/{}", self.base, self.room)
    }

    fn gone(&self) -> bool {
        let Self { base, room, secret } = self;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        matches!(
            agent
                .get(&format!("{base}/r/{room}/devices?secret={secret}"))
                .call(),
            Err(ureq::Error::StatusCode(403 | 404))
        )
    }
}

impl Link {
    fn call(&self, method: &str, action: &str, query: &str, body: &str) -> Option<String> {
        match self {
            Self::Here {
                hub, room, secret, ..
            } => {
                let mut query = pairs(query);
                query.push(("secret".to_string(), secret.clone()));
                let request = Request {
                    body: body.to_string(),
                    method: method.to_string(),
                    path: format!("/r/{room}/{action}"),
                    query,
                    ..Request::default()
                };
                let (status, _, text) = hub.handle(&request);
                status.starts_with("200").then_some(text)
            }
            Self::There(remote) => remote.call(method, action, query, body),
            Self::Cloud(remote, wire) => match (method, action) {
                ("GET", "in") => wire.next(),
                ("POST", "out") => {
                    wire.push(body.to_string());
                    Some("ok".to_string())
                }
                ("GET", "devices") => wire
                    .devices()
                    .or_else(|| remote.call(method, action, query, body)),
                _ => remote.call(method, action, query, body),
            },
        }
    }

    const fn reach(&self) -> Reach {
        match self {
            Self::Here { reach, .. } => *reach,
            Self::There(_) => Reach::Relay,
            Self::Cloud(..) => Reach::Hosted,
        }
    }

    fn page(&self) -> String {
        match self {
            Self::Here {
                room,
                address: (ip, port),
                ..
            } => format!("http://{ip}:{port}/r/{room}"),
            Self::There(remote) | Self::Cloud(remote, _) => remote.page(),
        }
    }
}

struct Session {
    keys: Mutex<([u8; 32], Option<[u8; 32]>)>,
    saved: Mutex<String>,
    link: Link,
    messages: Mutex<VecDeque<Arrival>>,
    answers: Mutex<VecDeque<(String, bool)>>,
    replies: Mutex<VecDeque<(String, Option<Vec<String>>)>>,
    stop: AtomicBool,
}

static LIVE: Mutex<Option<Arc<Session>>> = Mutex::new(None);
static HELD: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

pub fn hold(text: String) {
    if let Ok(mut held) = HELD.lock() {
        held.push_back(text);
    }
}

pub fn held() -> Option<String> {
    HELD.lock().ok()?.pop_front()
}

impl Session {
    fn key(&self) -> [u8; 32] {
        self.keys.lock().map(|keys| keys.0).unwrap_or_default()
    }

    fn open(&self, item: &str) -> Option<Vec<u8>> {
        let (current, previous) = self.keys.lock().ok().map(|keys| *keys)?;
        unseal(&current, item).or_else(|| previous.and_then(|previous| unseal(&previous, item)))
    }

    fn send(&self, key: &[u8; 32], message: &Value) {
        if let Some(sealed) = seal(key, message.to_string().as_bytes()) {
            let _ = self.link.call("POST", "out", "", &sealed);
        }
    }
}

pub fn route(to: Ipv4Addr) -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((to, 9)).ok()?;
    Some(socket.local_addr().ok()?.ip())
}

fn address() -> IpAddr {
    route(Ipv4Addr::new(192, 0, 2, 1)).unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

pub fn tailnet() -> Option<Node> {
    crate::tailscale::find()
}

const OFFLINE: &str = "tailscale isn't connected on this computer. turn it on and run /pair again, or pick home network in /pair";

fn live() -> Option<Arc<Session>> {
    LIVE.lock().ok()?.clone()
}

fn pull(session: &Session) {
    let mut after = 0;
    while !session.stop.load(Ordering::SeqCst) {
        let Some(body) = session
            .link
            .call("GET", "in", &format!("after={after}"), "")
        else {
            thread::sleep(Duration::from_secs(2));
            continue;
        };
        let Ok(reply) = serde_json::from_str::<Value>(&body) else {
            continue;
        };
        after = reply["next"]
            .as_u64()
            .and_then(|next| usize::try_from(next).ok())
            .unwrap_or(after);
        for item in reply["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            match session.open(item).as_deref().and_then(inbound) {
                Some(Inbound::Message { text, now, images }) => {
                    let images = stored(&images);
                    let text = marked(&text, images.len());
                    if let Ok(mut messages) = session.messages.lock() {
                        messages.push_back(Arrival { images, now, text });
                    }
                }
                Some(Inbound::Answer { id, yes }) => {
                    if let Ok(mut answers) = session.answers.lock() {
                        answers.push_back((id, yes));
                    }
                }
                Some(Inbound::Reply { id, answers }) => {
                    if let Ok(mut replies) = session.replies.lock() {
                        replies.push_back((id, answers));
                    }
                }
                Some(Inbound::Outdated) => post(&json!({
                    "kind": "note",
                    "text": "this page is out of date, so that answer was ignored. reload it and answer again",
                })),
                None => {}
            }
        }
    }
}

fn stored(images: &[Vec<u8>]) -> Vec<std::path::PathBuf> {
    let Some(roots) = sombre_core::Roots::detect() else {
        return Vec::new();
    };
    images
        .iter()
        .filter_map(|bytes| sombre_harness::image::store(bytes, &roots.sombre).ok())
        .collect()
}

pub fn marked(text: &str, count: usize) -> String {
    let marks: Vec<String> = (1..=count)
        .map(|number| format!("[Image #{number}]"))
        .collect();
    [text.to_string(), marks.join(" ")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn place() -> Option<std::path::PathBuf> {
    sombre_core::Roots::detect().map(|roots| roots.sombre.join("pair.json"))
}

fn snapshot(session: &Session) -> Value {
    let devices = session
        .link
        .call("GET", "devices", "", "")
        .and_then(|body| serde_json::from_str::<Value>(&body).ok())
        .unwrap_or_else(|| json!([]));
    let mut saved = json!({"devices": devices, "key": URL_SAFE_NO_PAD.encode(session.key())});
    match &session.link {
        Link::Here {
            room,
            secret,
            address: (ip, port),
            reach,
            ..
        } => {
            saved["room"] = json!(room);
            saved["secret"] = json!(secret);
            saved["ip"] = json!(ip.to_string());
            saved["port"] = json!(port);
            if *reach == Reach::Tailnet {
                saved["via"] = json!("tailnet");
            }
        }
        Link::There(remote) => {
            saved["room"] = json!(remote.room);
            saved["secret"] = json!(remote.secret);
            saved["relay"] = json!(remote.base);
        }
        Link::Cloud(remote, _) => {
            saved["room"] = json!(remote.room);
            saved["secret"] = json!(remote.secret);
            saved["hosted"] = json!(remote.base);
        }
    }
    saved
}

fn keep(session: &Session) {
    let Some(path) = place() else {
        return;
    };
    let text = snapshot(session).to_string();
    if session.saved.lock().is_ok_and(|saved| *saved == text) {
        return;
    }
    if path
        .parent()
        .is_some_and(|folder| std::fs::create_dir_all(folder).is_ok())
        && std::fs::write(&path, &text).is_ok()
        && sombre_harness::config::private(&path).is_ok()
        && let Ok(mut saved) = session.saved.lock()
    {
        *saved = text;
    }
}

fn start(link: Link, key: [u8; 32]) -> Arc<Session> {
    let pause = match link {
        Link::Here { .. } => Duration::from_secs(2),
        Link::There(_) | Link::Cloud(..) => Duration::from_secs(20),
    };
    let session = Arc::new(Session {
        answers: Mutex::new(VecDeque::new()),
        replies: Mutex::new(VecDeque::new()),
        keys: Mutex::new((key, None)),
        link,
        messages: Mutex::new(VecDeque::new()),
        saved: Mutex::new(String::new()),
        stop: AtomicBool::new(false),
    });
    let puller = Arc::clone(&session);
    thread::spawn(move || pull(&puller));
    let keeper = Arc::clone(&session);
    thread::spawn(move || {
        while !keeper.stop.load(Ordering::SeqCst) {
            keep(&keeper);
            thread::sleep(pause);
        }
    });
    if let Ok(mut live) = LIVE.lock() {
        *live = Some(Arc::clone(&session));
    }
    session
}

fn here(reach: Reach) -> Result<Link, String> {
    let link = match reach {
        Reach::Tailnet => {
            let node = tailnet().ok_or(OFFLINE)?;
            let mut tries = 0;
            loop {
                tries += 1;
                match here_at(node.ip(), node.bind(), reach).and_then(|link| expose(link, &node)) {
                    Err(error) if error.kind() == io::ErrorKind::AddrInUse && tries < 8 => {}
                    link => break link,
                }
            }
        }
        Reach::Wifi | Reach::Relay | Reach::Hosted => {
            let ip = address();
            here_at(ip, ip, reach)
        }
    };
    link.map_err(|error| format!("couldn't start pairing: {error}"))
}

fn expose(mut link: Link, node: &Node) -> io::Result<Link> {
    let (
        Node::Userspace { socket, .. },
        Link::Here {
            address: (_, port), ..
        },
    ) = (node, &link)
    else {
        return Ok(link);
    };
    if let Err(error) = crate::tailscale::forward(socket, *port) {
        halt(&link);
        return Err(error);
    }
    if let Link::Here { forwarded, .. } = &mut link {
        *forwarded = Some(socket.clone());
    }
    Ok(link)
}

fn here_at(ip: IpAddr, bind: IpAddr, reach: Reach) -> io::Result<Link> {
    let listener = TcpListener::bind((bind, 0))?;
    let port = listener.local_addr()?.port();
    let hub = Arc::new(Hub::new(false, None));
    let (room, secret) = hub
        .create()
        .ok_or_else(|| io::Error::other("couldn't make a room"))?;
    let serving = Arc::clone(&hub);
    thread::spawn(move || serving.listen(&listener));
    Ok(Link::Here {
        address: (ip, port),
        bind,
        forwarded: None,
        hub,
        reach,
        room,
        secret,
    })
}

fn made(base: &str, name: &str) -> Result<Remote, String> {
    let base = base.trim_end_matches('/').to_string();
    let password = std::env::var("SOMBRE_RELAY_PASSWORD").unwrap_or_default();
    let body = ureq::post(&format!("{base}/rooms?password={password}"))
        .send("")
        .map_err(|error| match error {
            ureq::Error::StatusCode(429) => {
                "too many new pairings from this network. wait a few minutes and try /pair again"
                    .to_string()
            }
            ureq::Error::StatusCode(403) => format!("{name} doesn't take new pairings"),
            _ => format!(
                "couldn't reach {name}. check your connection, or pick home network in /pair"
            ),
        })?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let reply: Value = serde_json::from_str(&body)
        .map_err(|_| format!("{name} sent something sombre couldn't read"))?;
    match (reply["room"].as_str(), reply["secret"].as_str()) {
        (Some(room), Some(secret)) => Ok(Remote {
            base,
            room: room.to_string(),
            secret: secret.to_string(),
        }),
        _ => Err(format!("{name} didn't make a room")),
    }
}

fn hosted() -> String {
    std::env::var("SOMBRE_HOSTED").unwrap_or_else(|_| HOSTED.to_string())
}

fn cloud(remote: Remote) -> Link {
    let wire = Wire::open(crate::wire::address(
        &remote.base,
        &remote.room,
        &remote.secret,
    ));
    Link::Cloud(remote, wire)
}

pub fn invite(reach: Reach, relay: Option<&str>) -> Result<(String, Reach), String> {
    let session = match live() {
        Some(session) => session,
        None => start(
            match (reach, relay) {
                (Reach::Hosted, _) => cloud(made(&hosted(), "sombre's relay")?),
                (Reach::Relay, Some(base)) => {
                    Link::There(made(base, &format!("the relay at {base}"))?)
                }
                (Reach::Relay, None) => return Err("there's no relay set".to_string()),
                (reach, _) => here(reach)?,
            },
            random(),
        ),
    };
    let token = hex(&random::<16>());
    session
        .link
        .call("POST", "token", "", &token)
        .ok_or("couldn't make a pairing link")?;
    Ok((
        format!(
            "{}#k={}&t={token}",
            session.link.page(),
            URL_SAFE_NO_PAD.encode(session.key())
        ),
        session.link.reach(),
    ))
}

pub fn close() -> bool {
    let Some(session) = LIVE.lock().ok().and_then(|mut live| live.take()) else {
        return false;
    };
    session.stop.store(true, Ordering::SeqCst);
    if let Some(path) = place() {
        let _ = std::fs::remove_file(path);
    }
    let _ = session.link.call("POST", "close", "", "");
    halt(&session.link);
    true
}

fn halt(link: &Link) {
    if let Link::Cloud(_, wire) = link {
        wire.stop();
    }
    if let Link::Here {
        hub,
        address: (_, port),
        bind,
        forwarded,
        ..
    } = link
    {
        hub.stop.store(true, Ordering::SeqCst);
        hub.changed.notify_all();
        let _ = TcpStream::connect_timeout(
            &std::net::SocketAddr::new(*bind, *port),
            Duration::from_secs(1),
        );
        if let Some(socket) = forwarded {
            let _ = crate::tailscale::unforward(socket, *port);
        }
    }
}

fn devices_from(saved: &Value) -> Vec<Device> {
    saved["devices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|device| {
            Some(Device {
                id: device["id"].as_str()?.to_string(),
                label: device["label"].as_str()?.to_string(),
                since: device["since"].as_u64()?,
            })
        })
        .collect()
}

pub fn resume() -> Result<bool, String> {
    let resumed = reopen();
    let saved = place()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    if let Some(port) = saved
        .filter(|saved| saved["via"] == "tailnet")
        .and_then(|saved| saved["port"].as_u64())
        .and_then(|port| u16::try_from(port).ok())
        && !forwarding(port)
        && TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
    {
        crate::tailscale::release(port);
    }
    resumed
}

fn forwarding(port: u16) -> bool {
    live().is_some_and(|session| {
        matches!(
            &session.link,
            Link::Here { forwarded: Some(_), address: (_, here), .. } if *here == port
        )
    })
}

fn reopen() -> Result<bool, String> {
    if live().is_some() {
        return Ok(false);
    }
    let Some(text) = place().and_then(|path| std::fs::read_to_string(path).ok()) else {
        return Ok(false);
    };
    let saved: Value = serde_json::from_str(&text).unwrap_or_default();
    let (Some(room), Some(secret)) = (saved["room"].as_str(), saved["secret"].as_str()) else {
        return Ok(false);
    };
    let Some(key) = saved["key"]
        .as_str()
        .and_then(|key| URL_SAFE_NO_PAD.decode(key).ok())
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
    else {
        return Ok(false);
    };
    let link = if let Some(base) = saved["relay"].as_str().or(saved["hosted"].as_str()) {
        let remote = Remote {
            base: base.to_string(),
            room: room.to_string(),
            secret: secret.to_string(),
        };
        if remote.gone() {
            return Err(
                "the relay no longer has your phone's pairing. /pair makes a new link".to_string(),
            );
        }
        if saved["hosted"].is_string() {
            cloud(remote)
        } else {
            Link::There(remote)
        }
    } else {
        let reach = if saved["via"] == "tailnet" {
            Reach::Tailnet
        } else {
            Reach::Wifi
        };
        let node = match reach {
            Reach::Tailnet => Some(tailnet().ok_or(
                "tailscale isn't connected, so your phone can't reach this computer. turn it on, or run /pair for a new link",
            )?),
            Reach::Wifi | Reach::Relay | Reach::Hosted => None,
        };
        let ip = node.as_ref().map_or_else(address, Node::ip);
        let bind = node.as_ref().map_or(ip, Node::bind);
        if saved["ip"].as_str() != Some(ip.to_string().as_str()) {
            return Err(match reach {
                Reach::Tailnet => "this computer's tailscale address changed, so your phone needs a new link. run /pair",
                Reach::Wifi | Reach::Relay | Reach::Hosted => "this computer's wi-fi address changed, so your phone needs a new link. run /pair",
            }
            .to_string());
        }
        let port = saved["port"]
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .unwrap_or_default();
        let Ok(listener) = TcpListener::bind((bind, port)) else {
            return Ok(false);
        };
        let hub = Arc::new(Hub::new(false, None));
        hub.restore(room, secret, devices_from(&saved));
        let serving = Arc::clone(&hub);
        thread::spawn(move || serving.listen(&listener));
        let link = Link::Here {
            address: (ip, port),
            bind,
            forwarded: None,
            hub,
            reach,
            room: room.to_string(),
            secret: secret.to_string(),
        };
        match &node {
            Some(node) => expose(link, node).map_err(|error| {
                format!("couldn't let your phone in through tailscale: {error}")
            })?,
            None => link,
        }
    };
    start(link, key);
    Ok(true)
}

pub fn paired() -> bool {
    live().is_some()
}

pub fn devices() -> Vec<Device> {
    live()
        .and_then(|session| session.link.call("GET", "devices", "", ""))
        .and_then(|body| serde_json::from_str::<Vec<Value>>(&body).ok())
        .unwrap_or_default()
        .iter()
        .filter_map(|device| {
            Some(Device {
                id: device["id"].as_str()?.to_string(),
                label: device["label"].as_str()?.to_string(),
                since: device["since"].as_u64()?,
            })
        })
        .collect()
}

pub fn forget(id: &str, again: &[Value]) -> bool {
    let Some(session) = live() else {
        return false;
    };
    if session.link.call("POST", "forget", "", id).is_none() {
        return false;
    }
    let old = session.key();
    let fresh: [u8; 32] = random();
    session.send(
        &old,
        &json!({"kind": "rekey", "key": URL_SAFE_NO_PAD.encode(fresh)}),
    );
    if let Ok(mut keys) = session.keys.lock() {
        *keys = (fresh, Some(old));
    }
    for item in again {
        let mut item = item.clone();
        item["again"] = json!(true);
        session.send(&fresh, &item);
    }
    true
}

pub fn post(message: &Value) {
    let Some(session) = live() else {
        return;
    };
    session.send(&session.key(), message);
}

pub fn message() -> Option<Arrival> {
    live()?.messages.lock().ok()?.pop_front()
}

fn pick<T: Clone>(queue: &mut VecDeque<(String, T)>, id: &str) -> Option<T> {
    let found = queue
        .iter()
        .find(|(asked, _)| asked == id)
        .map(|(_, value)| value.clone());
    queue.clear();
    found
}

pub fn answer(id: &str) -> Option<bool> {
    pick(&mut *live()?.answers.lock().ok()?, id)
}

pub fn replied(id: &str) -> Option<Option<Vec<String>>> {
    pick(&mut *live()?.replies.lock().ok()?, id)
}

fn served(name: &str, built: &str) -> String {
    std::env::var_os("SOMBRE_PHONE")
        .and_then(|folder| std::fs::read_to_string(Path::new(&folder).join(name)).ok())
        .unwrap_or_else(|| built.to_string())
}

fn picture(path: &Path) -> Value {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let small = std::fs::metadata(path).is_ok_and(|meta| meta.len() <= PICTURE);
    match small
        .then(|| sombre_harness::image::url(path).ok())
        .flatten()
    {
        Some(src) => json!({"name": name, "src": src}),
        None => json!({"name": name}),
    }
}

fn pictured(detail: &str) -> bool {
    let path = Path::new(detail);
    path.is_absolute()
        && path
            .extension()
            .and_then(|ending| ending.to_str())
            .is_some_and(|ending| crate::attach::ENDINGS.contains(&ending.to_lowercase().as_str()))
}

pub fn said(text: &str, images: &[std::path::PathBuf]) {
    if !paired() {
        return;
    }
    let images: Vec<Value> = images.iter().map(|path| picture(path)).collect();
    post(&json!({"kind": "user", "text": text, "images": images}));
}

pub fn mirror(item: &sombre_core::Event) {
    use sombre_core::Event;
    if !paired() {
        return;
    }
    if let Event::Step { label, detail, .. } = item
        && label == "Read"
        && pictured(detail)
    {
        post(&json!({"kind": "step", "label": label, "detail": detail}));
        post(&json!({"kind": "image", "image": picture(Path::new(detail))}));
        return;
    }
    let shown = match item {
        Event::Image(path) => json!({"kind": "image", "image": picture(Path::new(path))}),
        Event::Delta(text) => json!({"kind": "delta", "text": text}),
        Event::Text(text) => json!({"kind": "text", "text": text}),
        Event::Step { label, detail, .. } => {
            json!({"kind": "step", "label": label, "detail": detail})
        }
        Event::Output {
            text, failed: true, ..
        } => json!({"kind": "output", "text": text}),
        Event::Plan { steps, .. } => json!({"kind": "plan", "steps": steps}),
        Event::Approval {
            id,
            tool,
            detail,
            change,
            ..
        } => {
            json!({"kind": "approval", "id": id, "tool": tool, "detail": detail, "change": change})
        }
        Event::Questions { id, questions, .. } => json!({
            "kind": "questions",
            "id": id,
            "questions": questions
                .iter()
                .map(|question| json!({
                    "header": question.header,
                    "multiple": question.multiple,
                    "text": question.text,
                    "choices": question
                        .choices
                        .iter()
                        .map(|choice| json!({"label": choice.label, "about": choice.about}))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }),
        Event::Failed(text) => json!({"kind": "failed", "text": text}),
        Event::Note(text) => json!({"kind": "note", "text": text}),
        _ => return,
    };
    post(&shown);
}

pub fn serve(port: u16) -> io::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))?;
    let password = std::env::var("SOMBRE_RELAY_PASSWORD")
        .ok()
        .filter(|password| !password.is_empty());
    println!(
        "sombre relay listening on port {}. it only passes sealed messages and can't read them{}",
        listener.local_addr()?.port(),
        if password.is_some() {
            ". new rooms need SOMBRE_RELAY_PASSWORD"
        } else {
            ""
        }
    );
    Arc::new(Hub::new(true, password)).listen(&listener);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, path: &str, query: &[(&str, &str)], body: &str) -> Request {
        Request {
            body: body.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            query: query
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
            ..Request::default()
        }
    }

    fn session(link: Link) -> Session {
        Session {
            answers: Mutex::new(VecDeque::new()),
            replies: Mutex::new(VecDeque::new()),
            keys: Mutex::new(([7; 32], None)),
            link,
            messages: Mutex::new(VecDeque::new()),
            saved: Mutex::new(String::new()),
            stop: AtomicBool::new(false),
        }
    }

    #[test]
    fn a_tailnet_pairing_remembers_how_it_was_made() {
        let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let tailnet =
            here_at(local, local, Reach::Tailnet).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(tailnet.reach(), Reach::Tailnet);
        assert!(tailnet.page().starts_with("http://127.0.0.1:"));
        let saved = snapshot(&session(tailnet));
        assert_eq!(saved["via"], "tailnet");
        assert_eq!(saved["ip"], "127.0.0.1");
        let wifi = here_at(local, local, Reach::Wifi).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(wifi.reach(), Reach::Wifi);
        assert!(snapshot(&session(wifi)).get("via").is_none());
    }

    #[test]
    fn seals_so_only_the_key_opens_it() {
        let key: [u8; 32] = random();
        let sealed =
            seal(&key, b"{\"kind\":\"answer\",\"id\":\"r1\",\"yes\":true}").unwrap_or_default();
        assert!(!sealed.contains("answer"));
        assert_eq!(
            unseal(&key, &sealed).as_deref().and_then(inbound),
            Some(Inbound::Answer {
                id: "r1".to_string(),
                yes: true
            })
        );
        let other: [u8; 32] = random();
        assert!(unseal(&other, &sealed).is_none());
        assert!(unseal(&key, "short").is_none());
    }

    #[test]
    fn reads_phone_messages_and_cleans_them() {
        assert_eq!(
            inbound(br#"{"kind":"message","text":" fix it \u001b[2J ","now":true}"#),
            Some(Inbound::Message {
                images: Vec::new(),
                now: true,
                text: "fix it".to_string()
            })
        );
        assert_eq!(inbound(br#"{"kind":"message","text":"   "}"#), None);
        let png = STANDARD.encode([0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let photo = format!(
            r#"{{"kind":"message","text":"","images":[{{"data":"{png}"}},{{"data":"bm90IGFuIGltYWdl"}},{{"data":"%%%"}}]}}"#
        );
        assert_eq!(
            inbound(photo.as_bytes()),
            Some(Inbound::Message {
                images: vec![vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]],
                now: false,
                text: String::new()
            })
        );
        assert_eq!(inbound(br#"{"kind":"permissions","mode":"yolo"}"#), None);
        assert_eq!(
            inbound(br#"{"kind":"reply","id":"q1","answers":[" Ember ","Home, \u001b[2JDocs"]}"#),
            Some(Inbound::Reply {
                answers: Some(vec!["Ember".to_string(), "Home, Docs".to_string()]),
                id: "q1".to_string()
            })
        );
        assert_eq!(
            inbound(br#"{"kind":"reply","id":"q1","answers":null}"#),
            Some(Inbound::Reply {
                answers: None,
                id: "q1".to_string()
            })
        );
        assert_eq!(
            inbound(br#"{"kind":"answer","yes":true}"#),
            Some(Inbound::Outdated)
        );
        assert_eq!(
            inbound(br#"{"kind":"reply","answers":["Ink"]}"#),
            Some(Inbound::Outdated)
        );
    }

    #[test]
    fn an_answer_only_counts_for_the_prompt_it_was_sent_for() {
        let mut queue = VecDeque::from([("old".to_string(), true), ("now".to_string(), false)]);
        assert_eq!(pick(&mut queue, "now"), Some(false));
        assert!(queue.is_empty());
        let mut late = VecDeque::from([("old".to_string(), true)]);
        assert_eq!(pick(&mut late, "now"), None);
        assert!(late.is_empty());
    }

    #[test]
    fn numbers_phone_photos_like_pasted_images() {
        assert_eq!(
            marked("what is this", 2),
            "what is this [Image #1] [Image #2]"
        );
        assert_eq!(marked("", 1), "[Image #1]");
        assert_eq!(marked("plain", 0), "plain");
        assert!(stored(&[]).is_empty());
    }

    #[test]
    fn parses_requests_and_caps_bodies() {
        let raw = "POST /r/abc/send?device=abc&after=3 HTTP/1.1\r\nUser-Agent: Mozilla/5.0 (iPhone) Safari\r\nContent-Length: 5\r\n\r\nhello";
        let parsed = parse(&mut BufReader::new(raw.as_bytes())).unwrap_or_default();
        assert_eq!(parsed.path, "/r/abc/send");
        assert_eq!(parsed.get("device"), Some("abc"));
        assert_eq!(parsed.body, "hello");
        assert_eq!(label(&parsed.agent), "iPhone, Safari");
        let huge = format!(
            "POST /send HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            BODY + 1
        );
        assert!(parse(&mut BufReader::new(huge.as_bytes())).is_none());
    }

    #[test]
    fn restored_rooms_keep_their_phones_and_tell_old_pages_to_start_over() {
        let hub = Hub::new(false, None);
        let phone = Device {
            id: "phone-1".to_string(),
            label: "android".to_string(),
            since: 1,
        };
        hub.restore("room-1", "secret-1", vec![phone.clone()]);
        let path = |action: &str| format!("/r/room-1/{action}");
        let (code, _, _) = hub.handle(&request(
            "POST",
            &path("out"),
            &[("secret", "secret-1")],
            "sealed-1",
        ));
        assert!(code.starts_with("200"));
        let started = std::time::Instant::now();
        let (code, _, stale) = hub.handle(&request(
            "GET",
            &path("events"),
            &[("device", "phone-1"), ("after", "40")],
            "",
        ));
        assert!(code.starts_with("200"));
        assert!(started.elapsed() < Duration::from_secs(2));
        let stale: Value = serde_json::from_str(&stale).unwrap_or_default();
        assert_eq!(stale["epoch"].as_str().map(str::len), Some(12));
        let (_, _, fresh) = hub.handle(&request(
            "GET",
            &path("events"),
            &[("device", "phone-1"), ("after", "0")],
            "",
        ));
        assert!(fresh.contains("sealed-1"));
        assert!(
            hub.handle(&request(
                "GET",
                &path("events"),
                &[("device", "stranger"), ("after", "0")],
                "",
            ))
            .0
            .starts_with("403")
        );
        let saved = json!({"devices": [{"id": "phone-1", "label": "android", "since": 1}]});
        assert_eq!(devices_from(&saved), [phone]);
    }

    #[test]
    fn rooms_pair_once_and_forget_one_phone_at_a_time() {
        let hub = Hub::new(true, Some("open sesame".to_string()));
        assert!(
            hub.handle(&request("POST", "/rooms", &[], ""))
                .0
                .starts_with("403")
        );
        let (_, _, made) = hub.handle(&request(
            "POST",
            "/rooms",
            &[("password", "open sesame")],
            "",
        ));
        let made: Value = serde_json::from_str(&made).unwrap_or_default();
        let room = made["room"].as_str().unwrap_or_default().to_string();
        let secret = made["secret"].as_str().unwrap_or_default().to_string();
        let path = |action: &str| format!("/r/{room}/{action}");
        let status = |answer: Answer| answer.0;
        assert!(status(hub.handle(&request("POST", &path("token"), &[], "t1"))).starts_with("403"));
        assert!(
            status(hub.handle(&request(
                "POST",
                &path("token"),
                &[("secret", &secret)],
                "t1"
            )))
            .starts_with("200")
        );
        let (code, _, phone) = hub.handle(&request("POST", &path("pair"), &[], "t1"));
        assert!(code.starts_with("200"));
        assert!(status(hub.handle(&request("POST", &path("pair"), &[], "t1"))).starts_with("403"));
        assert!(
            status(hub.handle(&request(
                "POST",
                &path("out"),
                &[("secret", &secret)],
                "sealed-1"
            )))
            .starts_with("200")
        );
        let (_, _, events) = hub.handle(&request(
            "GET",
            &path("events"),
            &[("device", &phone), ("after", "0")],
            "",
        ));
        assert!(events.contains("sealed-1"));
        assert!(
            status(hub.handle(&request(
                "POST",
                &path("send"),
                &[("device", &phone)],
                "sealed-2"
            )))
            .starts_with("200")
        );
        for _ in 0..2 {
            assert!(
                status(hub.handle(&request(
                    "POST",
                    &path("send"),
                    &[("device", &phone), ("id", "m1")],
                    "sealed-3"
                )))
                .starts_with("200")
            );
        }
        let (_, _, inbox) = hub.handle(&request(
            "GET",
            &path("in"),
            &[("secret", &secret), ("after", "0")],
            "",
        ));
        assert!(inbox.contains("sealed-2"));
        assert_eq!(inbox.matches("sealed-3").count(), 1);
        assert!(
            status(hub.handle(&request("GET", &path("in"), &[("device", &phone)], "")))
                .starts_with("403")
        );
        assert!(
            status(hub.handle(&request(
                "POST",
                &path("forget"),
                &[("secret", &secret)],
                &phone
            )))
            .starts_with("200")
        );
        assert!(
            status(hub.handle(&request("POST", &path("send"), &[("device", &phone)], "x")))
                .starts_with("403")
        );
    }
}
