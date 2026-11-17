use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use tauri::ipc::Channel;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Out {
    Data { text: String },
    Exit { code: Option<u32> },
}

static OPENED: AtomicU64 = AtomicU64::new(0);

struct Session {
    id: u64,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Default)]
pub struct Terms(Arc<Mutex<HashMap<String, Session>>>);

#[cfg(unix)]
fn shell() -> CommandBuilder {
    let program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut command = CommandBuilder::new(program);
    command.arg("-l");
    command
}

#[cfg(windows)]
fn shell() -> CommandBuilder {
    CommandBuilder::new("powershell.exe")
}

fn release<T>(held: &mut HashMap<String, T>, name: &str, mine: impl Fn(&T) -> bool) -> Option<T> {
    if held.get(name).is_some_and(mine) {
        held.remove(name)
    } else {
        None
    }
}

pub fn decode(carry: &mut Vec<u8>) -> String {
    match std::str::from_utf8(carry) {
        Ok(text) => {
            let out = text.to_string();
            carry.clear();
            out
        }
        Err(error) if error.error_len().is_none() => {
            let cut = error.valid_up_to();
            let out = String::from_utf8_lossy(&carry[..cut]).into_owned();
            carry.drain(..cut);
            out
        }
        Err(_) => {
            let out = String::from_utf8_lossy(carry).into_owned();
            carry.clear();
            out
        }
    }
}

const fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols,
        pixel_height: 0,
        pixel_width: 0,
        rows,
    }
}

impl Terms {
    pub fn open(
        &self,
        key: &str,
        folder: &Path,
        cols: u16,
        rows: u16,
        out: Channel<Out>,
    ) -> Result<(), String> {
        let mut held = self
            .0
            .lock()
            .map_err(|_| "sombre lost track of its terminals")?;
        if held.contains_key(key) {
            return Ok(());
        }
        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .map_err(|error| format!("couldn't open a terminal: {error}"))?;
        let mut command = shell();
        command.cwd(folder);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("couldn't start your shell: {error}"))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("couldn't read the terminal: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("couldn't write to the terminal: {error}"))?;
        let id = OPENED.fetch_add(1, Ordering::Relaxed);
        held.insert(
            key.to_string(),
            Session {
                id,
                child,
                master: pair.master,
                writer,
            },
        );
        drop(held);
        let sessions = Arc::clone(&self.0);
        let name = key.to_string();
        thread::spawn(move || {
            let mut buffer = [0_u8; 8192];
            let mut carry = Vec::new();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        carry.extend_from_slice(&buffer[..read]);
                        let text = decode(&mut carry);
                        if !text.is_empty() && out.send(Out::Data { text }).is_err() {
                            break;
                        }
                    }
                }
            }
            let code = sessions
                .lock()
                .ok()
                .and_then(|mut held| release(&mut held, &name, |session| session.id == id))
                .and_then(|mut session| session.child.wait().ok())
                .map(|status| status.exit_code());
            let _ = out.send(Out::Exit { code });
        });
        Ok(())
    }

    pub fn write(&self, key: &str, data: &str) -> Result<(), String> {
        let mut held = self
            .0
            .lock()
            .map_err(|_| "sombre lost track of its terminals")?;
        let session = held
            .get_mut(key)
            .ok_or("this terminal closed. open a new one")?;
        session
            .writer
            .write_all(data.as_bytes())
            .and_then(|()| session.writer.flush())
            .map_err(|error| format!("couldn't type into the terminal: {error}"))
    }

    pub fn resize(&self, key: &str, cols: u16, rows: u16) -> Result<(), String> {
        let held = self
            .0
            .lock()
            .map_err(|_| "sombre lost track of its terminals")?;
        if let Some(session) = held.get(key) {
            session
                .master
                .resize(size(cols.max(2), rows.max(1)))
                .map_err(|error| format!("couldn't resize the terminal: {error}"))?;
        }
        Ok(())
    }

    pub fn hangup(&self, key: &str) {
        let removed = self.0.lock().ok().and_then(|mut held| held.remove(key));
        if let Some(mut session) = removed {
            let _ = session.child.kill();
        }
    }

    pub fn close(&self) {
        let Ok(mut held) = self.0.lock() else {
            return;
        };
        for (_, mut session) in held.drain() {
            let _ = session.child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_shell_only_releases_its_own_session() {
        let mut held = HashMap::from([("chat".to_string(), 2_u64)]);
        assert_eq!(release(&mut held, "chat", |id| *id == 1), None);
        assert_eq!(held.len(), 1);
        assert_eq!(release(&mut held, "chat", |id| *id == 2), Some(2));
        assert!(held.is_empty());
    }

    #[test]
    fn keeps_a_split_character_for_the_next_chunk() {
        let bytes = "ok ✓".as_bytes();
        let mut carry = bytes[..bytes.len() - 1].to_vec();
        assert_eq!(decode(&mut carry), "ok ");
        assert_eq!(carry.len(), 2);
        carry.push(bytes[bytes.len() - 1]);
        assert_eq!(decode(&mut carry), "✓");
        assert!(carry.is_empty());
        let mut broken = vec![b'a', 0xff, b'b'];
        assert_eq!(decode(&mut broken), "a\u{fffd}b");
    }
}
