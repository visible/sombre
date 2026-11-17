use std::io;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const SOCKETS: [&str; 2] = [
    "/var/run/tailscaled.socket",
    "/run/tailscale/tailscaled.sock",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Kernel(IpAddr),
    Userspace { ip: IpAddr, socket: PathBuf },
}

impl Node {
    pub const fn ip(&self) -> IpAddr {
        match self {
            Self::Kernel(ip) | Self::Userspace { ip, .. } => *ip,
        }
    }

    pub const fn bind(&self) -> IpAddr {
        match self {
            Self::Kernel(ip) => *ip,
            Self::Userspace { .. } => IpAddr::V4(Ipv4Addr::LOCALHOST),
        }
    }
}

pub fn find() -> Option<Node> {
    crate::route(Ipv4Addr::new(100, 100, 100, 100))
        .filter(|ip| tailnet(*ip))
        .map(Node::Kernel)
        .or_else(|| sockets().into_iter().find_map(|socket| ask(&socket)))
}

fn tailnet(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [first, second, ..] = ip.octets();
            first == 100 && (64..128).contains(&second) && ip != Ipv4Addr::new(100, 100, 100, 100)
        }
        IpAddr::V6(_) => false,
    }
}

fn sockets() -> Vec<PathBuf> {
    let listed = Command::new("ps")
        .args(["-axww", "-o", "command="])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let mut found: Vec<PathBuf> = Vec::new();
    for path in listed
        .lines()
        .filter_map(flag)
        .chain(SOCKETS.iter().map(PathBuf::from))
    {
        if path.exists() && !found.contains(&path) {
            found.push(path);
        }
    }
    found
}

fn flag(command: &str) -> Option<PathBuf> {
    let mut words = command.split_whitespace();
    let program = words.next()?;
    if !program.ends_with("tailscaled") {
        return None;
    }
    while let Some(word) = words.next() {
        let word = word.trim_start_matches('-');
        if let Some(path) = word.strip_prefix("socket=") {
            return Some(PathBuf::from(path));
        }
        if word == "socket" {
            return words.next().map(PathBuf::from);
        }
    }
    None
}

fn ask(socket: &Path) -> Option<Node> {
    let (_, body) = local(socket, "GET", "status", &[], "").ok()?;
    let status: Value = serde_json::from_str(&body).ok()?;
    if status["BackendState"] != "Running" {
        return None;
    }
    let ip = status["Self"]["TailscaleIPs"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|ip| ip.parse::<IpAddr>().ok())
        .find(|ip| tailnet(*ip))?;
    Some(if status["TUN"] == true {
        Node::Kernel(ip)
    } else {
        Node::Userspace {
            ip,
            socket: socket.to_path_buf(),
        }
    })
}

pub fn forward(socket: &Path, port: u16) -> io::Result<()> {
    serve(socket, |config| {
        let tcp = config
            .as_object_mut()
            .ok_or_else(|| io::Error::other("tailscale sent a serve config sombre couldn't read"))?
            .entry("TCP")
            .or_insert_with(|| json!({}));
        if !tcp.is_object() {
            *tcp = json!({});
        }
        let ours = json!({"TCPForward": format!("127.0.0.1:{port}")});
        match tcp.get(port.to_string()) {
            Some(entry) if *entry == ours => Ok(false),
            Some(_) => Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("tailscale serve already uses port {port}"),
            )),
            None => {
                tcp[port.to_string()] = ours;
                Ok(true)
            }
        }
    })
}

pub fn release(port: u16) {
    for socket in sockets() {
        let _ = unforward(&socket, port);
    }
}

pub fn unforward(socket: &Path, port: u16) -> io::Result<()> {
    serve(socket, |config| {
        let ours = json!({"TCPForward": format!("127.0.0.1:{port}")});
        Ok(config["TCP"]
            .as_object_mut()
            .filter(|tcp| tcp.get(&port.to_string()) == Some(&ours))
            .and_then(|tcp| tcp.remove(&port.to_string()))
            .is_some())
    })
}

fn serve(socket: &Path, change: impl Fn(&mut Value) -> io::Result<bool>) -> io::Result<()> {
    let mut tries = 0;
    loop {
        tries += 1;
        match swap(socket, &change) {
            Err(error) if error.kind() == io::ErrorKind::ResourceBusy && tries < 3 => {}
            done => return done,
        }
    }
}

fn swap(socket: &Path, change: &impl Fn(&mut Value) -> io::Result<bool>) -> io::Result<()> {
    let (headers, body) = local(socket, "GET", "serve-config", &[], "")?;
    let tag = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("etag")
            .then(|| value.trim().to_string())
    });
    let body = if body.trim().is_empty() {
        "null"
    } else {
        body.as_str()
    };
    let mut config: Value = serde_json::from_str(body)
        .map_err(|_| io::Error::other("tailscale sent a serve config sombre couldn't read"))?;
    if config.is_null() {
        config = json!({});
    }
    if !config.is_object() {
        return Err(io::Error::other(
            "tailscale sent a serve config sombre couldn't read",
        ));
    }
    if !change(&mut config)? {
        return Ok(());
    }
    let matching = tag.as_deref().map(|tag| ("If-Match", tag));
    local(
        socket,
        "POST",
        "serve-config",
        matching.as_slice(),
        &config.to_string(),
    )
    .map(|_| ())
}

#[cfg(not(unix))]
fn local(
    _socket: &Path,
    _method: &str,
    _action: &str,
    _extra: &[(&str, &str)],
    _body: &str,
) -> io::Result<(String, String)> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(unix)]
fn local(
    socket: &Path,
    method: &str,
    action: &str,
    extra: &[(&str, &str)],
    body: &str,
) -> io::Result<(String, String)> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut request = format!(
        "{method} /localapi/v0/{action} HTTP/1.0\r\nHost: local-tailscaled.sock\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in extra {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes())?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    let (head, rest) = reply
        .split_once("\r\n\r\n")
        .ok_or_else(|| io::Error::other("tailscale sent an incomplete reply"))?;
    let status = head.lines().next().unwrap_or_default();
    let code = status.split_whitespace().nth(1).unwrap_or_default();
    if !code.starts_with('2') {
        let kind = if code == "412" {
            io::ErrorKind::ResourceBusy
        } else {
            io::ErrorKind::Other
        };
        return Err(io::Error::new(
            kind,
            format!(
                "tailscale said {}: {}",
                status.split_once(' ').map_or(status, |(_, rest)| rest),
                rest.trim()
            ),
        ));
    }
    Ok((head.to_string(), rest.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::io::{Read, Write};
    #[cfg(unix)]
    use std::os::unix::net::UnixStream;

    #[test]
    fn knows_a_tailscale_address_when_it_sees_one() {
        let on = |a, b, c, d| tailnet(IpAddr::V4(Ipv4Addr::new(a, b, c, d)));
        assert!(on(100, 101, 102, 103));
        assert!(on(100, 64, 0, 1));
        assert!(on(100, 127, 255, 254));
        assert!(!on(100, 63, 0, 1));
        assert!(!on(100, 128, 0, 1));
        assert!(!on(100, 100, 100, 100));
        assert!(!on(192, 168, 1, 217));
        assert!(!tailnet(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn reads_the_socket_a_tailscaled_was_started_with() {
        let path = |command: &str| flag(command).map(|path| path.display().to_string());
        assert_eq!(
            path(
                "/opt/homebrew/bin/tailscaled --tun=userspace-networking --socket=/tmp/ts.sock --port=0"
            )
            .as_deref(),
            Some("/tmp/ts.sock")
        );
        assert_eq!(
            path("tailscaled -socket /run/ts.sock").as_deref(),
            Some("/run/ts.sock")
        );
        assert_eq!(path("/usr/sbin/tailscaled --state=mem:"), None);
        assert_eq!(path("tailscale --socket=/tmp/ts.sock status"), None);
    }

    #[test]
    fn a_userspace_node_listens_on_localhost() {
        let ip = IpAddr::V4(Ipv4Addr::new(100, 88, 85, 53));
        let node = Node::Userspace {
            ip,
            socket: PathBuf::from("/tmp/ts.sock"),
        };
        assert_eq!(node.ip(), ip);
        assert_eq!(node.bind(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(Node::Kernel(ip).bind(), ip);
    }

    #[cfg(unix)]
    fn fake(
        config: &str,
    ) -> (
        tempfile::TempDir,
        PathBuf,
        std::thread::JoinHandle<Vec<String>>,
    ) {
        use std::io::{BufRead, BufReader};
        let folder = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let socket = folder.path().join("tailscaled.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket)
            .unwrap_or_else(|error| panic!("{error}"));
        let config = config.to_string();
        let server = std::thread::spawn(move || {
            let mut posted = Vec::new();
            for _ in 0..2 {
                let Ok((stream, _)) = listener.accept() else {
                    break;
                };
                let mut reader = BufReader::new(stream);
                let mut head = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.strip_prefix("Content-Length: ") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    head.push_str(&line);
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let mut stream = reader.into_inner();
                if head.starts_with("POST") {
                    assert!(head.contains("If-Match: \"tag\""), "{head}");
                    posted.push(String::from_utf8_lossy(&body).into_owned());
                    let _ = stream.write_all(b"HTTP/1.0 200 OK\r\n\r\n");
                    break;
                }
                let _ = write!(stream, "HTTP/1.0 200 OK\r\nEtag: \"tag\"\r\n\r\n{config}");
            }
            posted
        });
        (folder, socket, server)
    }

    #[cfg(unix)]
    fn posted(server: std::thread::JoinHandle<Vec<String>>) -> Option<Value> {
        server
            .join()
            .unwrap_or_default()
            .first()
            .and_then(|body| serde_json::from_str(body).ok())
    }

    #[cfg(unix)]
    #[test]
    fn forwards_a_port_and_keeps_what_was_served() {
        let (_folder, socket, server) =
            fake(r#"{"TCP":{"443":{"HTTPS":true}},"Web":{"mac:443":{}}}"#);
        assert!(forward(&socket, 4000).is_ok());
        let config = posted(server).unwrap_or_default();
        assert_eq!(config["TCP"]["4000"]["TCPForward"], "127.0.0.1:4000");
        assert_eq!(config["TCP"]["443"]["HTTPS"], true);
        assert!(config["Web"]["mac:443"].is_object());
    }

    #[cfg(unix)]
    #[test]
    fn forwards_into_an_empty_config() {
        let (_folder, socket, server) = fake("null");
        assert!(forward(&socket, 4000).is_ok());
        let config = posted(server).unwrap_or_default();
        assert_eq!(config["TCP"]["4000"]["TCPForward"], "127.0.0.1:4000");
    }

    #[cfg(unix)]
    #[test]
    fn wont_take_a_port_something_else_serves() {
        let (_folder, socket, server) = fake(r#"{"TCP":{"4000":{"TCPForward":"127.0.0.1:9999"}}}"#);
        let error = forward(&socket, 4000).err();
        assert_eq!(
            error.map(|error| error.kind()),
            Some(io::ErrorKind::AddrInUse)
        );
        drop(UnixStream::connect(&socket));
        assert_eq!(posted(server), None);
    }

    #[cfg(unix)]
    #[test]
    fn leaves_a_config_it_cant_read_alone() {
        let (_folder, socket, server) = fake(r#"{"TCP":{"443":"#);
        assert!(forward(&socket, 4000).is_err());
        drop(UnixStream::connect(&socket));
        assert_eq!(posted(server), None);
    }

    #[cfg(unix)]
    #[test]
    fn unforwards_only_its_own_entry() {
        let (_folder, socket, server) = fake(
            r#"{"TCP":{"4000":{"TCPForward":"127.0.0.1:4000"},"53101":{"TCPForward":"127.0.0.1:63101"}}}"#,
        );
        assert!(unforward(&socket, 4000).is_ok());
        let config = posted(server).unwrap_or_default();
        assert!(config["TCP"].get("4000").is_none());
        assert_eq!(config["TCP"]["53101"]["TCPForward"], "127.0.0.1:63101");

        let (_folder, socket, server) = fake(r#"{"TCP":{"4000":{"TCPForward":"127.0.0.1:9999"}}}"#);
        assert!(unforward(&socket, 4000).is_ok());
        drop(UnixStream::connect(&socket));
        assert_eq!(posted(server), None);
    }
}
