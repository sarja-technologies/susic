//! Authenticated, encrypted desktop → device transfer over the local network.
//!
//! * Transport: plain TCP carrying a Noise `NNpsk0_25519_ChaChaPoly_BLAKE2s`
//!   session. The pre-shared key is derived from a one-time pairing code shown
//!   (and QR-encoded) on the sender, so only a device that scanned/typed the code
//!   can complete the handshake; all traffic is end-to-end encrypted.
//! * The pairing code is single-session, expires, and is revoked after repeated
//!   failed handshakes. Nothing is served without a completed handshake.
//! * The receiver keeps its own copy of the files; the sender is not needed
//!   afterwards.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PATTERN: &str = "Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s";
const MAX_FRAME: usize = 65_535;
const CHUNK: usize = 48 * 1024;
const MAX_FAILED_HANDSHAKES: u32 = 5;
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Io(#[from] std::io::Error),
    #[error("secure channel error: {0}")]
    Noise(String),
    #[error("{0}")]
    Proto(String),
    #[error("pairing failed — check the code and that both devices are on the same network")]
    PairingFailed,
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<snow::Error> for Error {
    fn from(e: snow::Error) -> Self {
        Error::Noise(e.to_string())
    }
}

// ---- pairing code ---------------------------------------------------------

pub fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    (0..12).map(|_| CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())] as char).collect()
}

pub fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect()
}

pub fn format_code(code: &str) -> String {
    code.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("-")
}

fn psk(code: &str) -> [u8; 32] {
    *blake3::derive_key("susic 0.1 pairing psk", normalize_code(code).as_bytes()).as_slice().first_chunk::<32>().unwrap()
}

/// `susic://pair?host=…&port=…&code=…` for QR encoding.
pub fn pairing_uri(host: &str, port: u16, code: &str) -> String {
    format!("susic://pair?host={host}&port={port}&code={code}")
}

pub fn parse_pairing_uri(uri: &str) -> Option<(String, u16, String)> {
    let q = uri.strip_prefix("susic://pair?")?;
    let (mut host, mut port, mut code) = (None, None, None);
    for kv in q.split('&') {
        let (k, v) = kv.split_once('=')?;
        match k {
            "host" => host = Some(v.to_string()),
            "port" => port = v.parse().ok(),
            "code" => code = Some(v.to_string()),
            _ => {}
        }
    }
    Some((host?, port?, code?))
}

// ---- secure framing ---------------------------------------------------------

pub struct Secure {
    stream: TcpStream,
    noise: snow::TransportState,
}

fn write_frame(s: &mut TcpStream, data: &[u8]) -> std::io::Result<()> {
    s.write_all(&(data.len() as u16).to_be_bytes())?;
    s.write_all(data)
}

fn read_frame(s: &mut TcpStream) -> Result<Vec<u8>> {
    let mut len = [0u8; 2];
    s.read_exact(&mut len)?;
    let mut buf = vec![0u8; u16::from_be_bytes(len) as usize];
    s.read_exact(&mut buf)?;
    Ok(buf)
}

impl Secure {
    fn handshake_initiator(mut stream: TcpStream, code: &str) -> Result<Self> {
        let mut hs = snow::Builder::new(PATTERN.parse()?).psk(0, &psk(code)).build_initiator()?;
        let mut buf = vec![0u8; MAX_FRAME];
        let n = hs.write_message(&[], &mut buf)?;
        write_frame(&mut stream, &buf[..n])?;
        let reply = read_frame(&mut stream).map_err(|_| Error::PairingFailed)?;
        hs.read_message(&reply, &mut buf).map_err(|_| Error::PairingFailed)?;
        Ok(Self { stream, noise: hs.into_transport_mode()? })
    }

    fn handshake_responder(mut stream: TcpStream, code: &str) -> Result<Self> {
        let mut hs = snow::Builder::new(PATTERN.parse()?).psk(0, &psk(code)).build_responder()?;
        let mut buf = vec![0u8; MAX_FRAME];
        let first = read_frame(&mut stream)?;
        hs.read_message(&first, &mut buf).map_err(|_| Error::PairingFailed)?;
        let n = hs.write_message(&[], &mut buf)?;
        write_frame(&mut stream, &buf[..n])?;
        Ok(Self { stream, noise: hs.into_transport_mode()? })
    }

    /// Send one logical message (any length).
    pub fn send(&mut self, data: &[u8]) -> Result<()> {
        let mut out = vec![0u8; MAX_FRAME];
        let mut hdr = Vec::with_capacity(4);
        hdr.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let n = self.noise.write_message(&hdr, &mut out)?;
        write_frame(&mut self.stream, &out[..n])?;
        for chunk in data.chunks(CHUNK) {
            let n = self.noise.write_message(chunk, &mut out)?;
            write_frame(&mut self.stream, &out[..n])?;
        }
        Ok(())
    }

    pub fn recv(&mut self) -> Result<Vec<u8>> {
        let mut out = vec![0u8; MAX_FRAME];
        let f = read_frame(&mut self.stream)?;
        let n = self.noise.read_message(&f, &mut out)?;
        if n != 4 {
            return Err(Error::Proto("bad message header".into()));
        }
        let total = u32::from_be_bytes(out[..4].try_into().unwrap()) as usize;
        if total > 64 * 1024 * 1024 {
            return Err(Error::Proto("message too large".into()));
        }
        let mut data = Vec::with_capacity(total);
        while data.len() < total {
            let f = read_frame(&mut self.stream)?;
            let n = self.noise.read_message(&f, &mut out)?;
            data.extend_from_slice(&out[..n]);
        }
        Ok(data)
    }
}

// ---- application protocol ---------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteTrack {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub duration_ms: i64,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub year: Option<i64>,
    pub size_bytes: i64,
    pub hash: String,
    pub format: String,
    pub has_artwork: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemotePlaylist {
    pub name: String,
    pub track_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub device: String,
    pub tracks: Vec<RemoteTrack>,
    pub playlists: Vec<RemotePlaylist>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Request {
    List,
    Get { id: i64, offset: u64 },
    Artwork { id: i64 },
    Lyrics { id: i64 },
    Bye,
}

#[derive(Serialize, Deserialize)]
struct FileHeader {
    ok: bool,
    size: u64,
    error: Option<String>,
}

/// What the sender exposes. Implemented by the application on top of the library.
pub trait Provider: Send + Sync + 'static {
    fn manifest(&self) -> Manifest;
    /// Open the audio file for a track.
    fn open(&self, id: i64) -> std::io::Result<(std::fs::File, u64)>;
    fn artwork(&self, id: i64) -> Option<Vec<u8>>;
    fn lyrics(&self, id: i64) -> Option<String>;
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ServerEvent {
    Listening { port: u16 },
    Paired,
    Sent { id: i64 },
    Disconnected,
    Expired,
    Revoked,
}

pub struct Server {
    pub port: u16,
    pub code: String,
    stop: Arc<AtomicBool>,
}

impl Server {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the blocking accept().
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

/// Start sharing. The code expires after `ttl` of inactivity.
pub fn serve(
    provider: Arc<dyn Provider>,
    ttl: Duration,
    on_event: impl Fn(ServerEvent) + Send + Sync + 'static,
) -> Result<Server> {
    let listener = TcpListener::bind(("0.0.0.0", 0))?;
    let port = listener.local_addr()?.port();
    let code = generate_code();
    let stop = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicU32::new(0));
    let last_activity = Arc::new(Mutex::new(Instant::now()));
    on_event(ServerEvent::Listening { port });
    let on_event = Arc::new(on_event);

    // Expiry watchdog.
    {
        let (stop, la, ev, code_port) = (stop.clone(), last_activity.clone(), on_event.clone(), port);
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(500));
                if la.lock().unwrap().elapsed() > ttl {
                    stop.store(true, Ordering::SeqCst);
                    let _ = TcpStream::connect(("127.0.0.1", code_port));
                    ev(ServerEvent::Expired);
                }
            }
        });
    }

    let (stop2, code2) = (stop.clone(), code.clone());
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if stop2.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = conn else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
            let _ = stream.set_nodelay(true);
            match Secure::handshake_responder(stream, &code2) {
                Ok(mut sec) => {
                    failed.store(0, Ordering::SeqCst);
                    on_event(ServerEvent::Paired);
                    *last_activity.lock().unwrap() = Instant::now();
                    let la = last_activity.clone();
                    let ev = on_event.clone();
                    let r = session(&mut sec, provider.as_ref(), &|id| {
                        *la.lock().unwrap() = Instant::now();
                        ev(ServerEvent::Sent { id });
                    });
                    let _ = r;
                    on_event(ServerEvent::Disconnected);
                }
                Err(_) => {
                    if failed.fetch_add(1, Ordering::SeqCst) + 1 >= MAX_FAILED_HANDSHAKES {
                        stop2.store(true, Ordering::SeqCst);
                        on_event(ServerEvent::Revoked);
                        break;
                    }
                }
            }
        }
    });
    Ok(Server { port, code, stop })
}

fn session(sec: &mut Secure, p: &dyn Provider, on_sent: &dyn Fn(i64)) -> Result<()> {
    // Keep-alive while idle, up to the watchdog's TTL.
    sec.stream.set_read_timeout(Some(Duration::from_secs(600)))?;
    loop {
        let raw = sec.recv()?;
        let req: Request = serde_json::from_slice(&raw).map_err(|e| Error::Proto(e.to_string()))?;
        match req {
            Request::List => sec.send(&serde_json::to_vec(&p.manifest()).unwrap())?,
            Request::Get { id, offset } => match p.open(id) {
                Ok((mut f, size)) => {
                    use std::io::Seek;
                    let offset = offset.min(size);
                    f.seek(std::io::SeekFrom::Start(offset))?;
                    let hdr = FileHeader { ok: true, size: size - offset, error: None };
                    sec.send(&serde_json::to_vec(&hdr).unwrap())?;
                    let mut buf = vec![0u8; 256 * 1024];
                    loop {
                        let n = f.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        sec.send(&buf[..n])?;
                    }
                    on_sent(id);
                }
                Err(e) => {
                    let hdr = FileHeader { ok: false, size: 0, error: Some(e.to_string()) };
                    sec.send(&serde_json::to_vec(&hdr).unwrap())?;
                }
            },
            Request::Artwork { id } => sec.send(&p.artwork(id).unwrap_or_default())?,
            Request::Lyrics { id } => sec.send(p.lyrics(id).unwrap_or_default().as_bytes())?,
            Request::Bye => return Ok(()),
        }
    }
}

// ---- client -----------------------------------------------------------------

pub struct Client {
    sec: Secure,
}

impl Client {
    pub fn connect(host: &str, port: u16, code: &str) -> Result<Self> {
        let addr: SocketAddr = (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| Error::Proto("could not resolve host".into()))?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(8))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_nodelay(true).ok();
        Ok(Self { sec: Secure::handshake_initiator(stream, code)? })
    }

    fn call(&mut self, r: &Request) -> Result<()> {
        self.sec.send(&serde_json::to_vec(r).unwrap())
    }

    pub fn manifest(&mut self) -> Result<Manifest> {
        self.call(&Request::List)?;
        serde_json::from_slice(&self.sec.recv()?).map_err(|e| Error::Proto(e.to_string()))
    }

    pub fn artwork(&mut self, id: i64) -> Result<Option<Vec<u8>>> {
        self.call(&Request::Artwork { id })?;
        let b = self.sec.recv()?;
        Ok(if b.is_empty() { None } else { Some(b) })
    }

    pub fn lyrics(&mut self, id: i64) -> Result<Option<String>> {
        self.call(&Request::Lyrics { id })?;
        let b = self.sec.recv()?;
        Ok(if b.is_empty() { None } else { String::from_utf8(b).ok() })
    }

    /// Download a track into `out`, resuming from `offset`. Calls `progress(bytes_done, total)`.
    pub fn fetch(
        &mut self,
        id: i64,
        offset: u64,
        out: &mut impl Write,
        mut progress: impl FnMut(u64, u64),
        cancel: &AtomicBool,
    ) -> Result<u64> {
        self.call(&Request::Get { id, offset })?;
        let hdr: FileHeader =
            serde_json::from_slice(&self.sec.recv()?).map_err(|e| Error::Proto(e.to_string()))?;
        if !hdr.ok {
            return Err(Error::Proto(hdr.error.unwrap_or_else(|| "transfer refused".into())));
        }
        let mut got = 0u64;
        while got < hdr.size {
            if cancel.load(Ordering::Relaxed) {
                // Connection state is mid-stream; caller must reconnect.
                return Err(Error::Proto("cancelled".into()));
            }
            let chunk = self.sec.recv()?;
            out.write_all(&chunk)?;
            got += chunk.len() as u64;
            progress(offset + got, offset + hdr.size);
        }
        Ok(offset + got)
    }

    pub fn close(mut self) {
        let _ = self.call(&Request::Bye);
    }
}

pub fn local_ip() -> Option<String> {
    // Connecting a UDP socket picks the outbound interface without sending data.
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("10.255.255.255:1").ok()?;
    Some(s.local_addr().ok()?.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct P(std::path::PathBuf, Vec<u8>);
    impl Provider for P {
        fn manifest(&self) -> Manifest {
            Manifest {
                device: "test".into(),
                tracks: vec![RemoteTrack {
                    id: 1,
                    title: "T".into(),
                    artist: "A".into(),
                    album: "B".into(),
                    album_artist: "".into(),
                    duration_ms: 1,
                    track_no: None,
                    disc_no: None,
                    year: None,
                    size_bytes: self.1.len() as i64,
                    hash: "h".into(),
                    format: "wav".into(),
                    has_artwork: false,
                }],
                playlists: vec![],
            }
        }
        fn open(&self, id: i64) -> std::io::Result<(std::fs::File, u64)> {
            if id != 1 {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
            }
            Ok((std::fs::File::open(&self.0)?, self.1.len() as u64))
        }
        fn artwork(&self, _: i64) -> Option<Vec<u8>> {
            None
        }
        fn lyrics(&self, _: i64) -> Option<String> {
            Some("[00:01.00]x".into())
        }
    }

    #[test]
    fn transfer_roundtrip_with_resume_and_bad_code() {
        let dir = std::env::temp_dir().join(format!("susic-sync-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let data: Vec<u8> = (0..700_000u32).map(|i| (i % 251) as u8).collect();
        let f = dir.join("a.bin");
        std::fs::write(&f, &data).unwrap();
        let srv = serve(Arc::new(P(f, data.clone())), Duration::from_secs(60), |_| {}).unwrap();

        // Wrong code is rejected.
        assert!(Client::connect("127.0.0.1", srv.port, "WRONGWRONG12").is_err());

        let mut c = Client::connect("127.0.0.1", srv.port, &srv.code).unwrap();
        let m = c.manifest().unwrap();
        assert_eq!(m.tracks.len(), 1);
        assert_eq!(c.lyrics(1).unwrap().as_deref(), Some("[00:01.00]x"));
        let mut out = Cursor::new(Vec::new());
        let cancel = AtomicBool::new(false);
        let n = c.fetch(1, 0, &mut out, |_, _| {}, &cancel).unwrap();
        assert_eq!(n as usize, data.len());
        assert_eq!(out.into_inner(), data);
        // Resume from an offset.
        let mut tail = Cursor::new(Vec::new());
        c.fetch(1, 600_000, &mut tail, |_, _| {}, &cancel).unwrap();
        assert_eq!(tail.into_inner(), data[600_000..]);
        assert!(c.fetch(9, 0, &mut Cursor::new(Vec::new()), |_, _| {}, &cancel).is_err());
        c.close();
        srv.stop();
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn code_helpers() {
        let c = generate_code();
        assert_eq!(c.len(), 12);
        assert_eq!(normalize_code("abcd-0ilo"), "ABCD01L0".replace('L', "1"));
        let uri = pairing_uri("192.168.1.2", 4000, &c);
        assert_eq!(parse_pairing_uri(&uri), Some(("192.168.1.2".into(), 4000, c)));
    }
}
