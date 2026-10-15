//! A `Read + Seek` view of a remote file using HTTP Range requests, with
//! one-chunk read-ahead and automatic reconnection. Lets the audio decoder
//! stream and seek a server track without downloading it first.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const CHUNK: u64 = 512 * 1024;
const MAX_ATTEMPTS: u32 = 6;

struct Inner {
    url: String,
    agent: ureq::Agent,
}

impl Inner {
    fn fetch(&self, start: u64, len: u64) -> io::Result<(Vec<u8>, Option<u64>)> {
        let mut attempt = 0;
        loop {
            let res = (|| -> Result<(Vec<u8>, Option<u64>), String> {
                let r = self
                    .agent
                    .get(&self.url)
                    .set("Range", &format!("bytes={}-{}", start, start + len - 1))
                    .call()
                    .map_err(|e| match e {
                        ureq::Error::Status(c, _) if c < 500 => format!("fatal:HTTP {c}"),
                        e => e.to_string(),
                    })?;
                let total = r
                    .header("Content-Range")
                    .and_then(|c| c.rsplit('/').next())
                    .and_then(|n| n.parse::<u64>().ok());
                let mut buf = Vec::with_capacity(len as usize);
                r.into_reader()
                    .take(len)
                    .read_to_end(&mut buf)
                    .map_err(|e| e.to_string())?;
                Ok((buf, total))
            })();
            match res {
                Ok(v) => return Ok(v),
                Err(e) if e.starts_with("fatal:") => return Err(io::Error::other(e)),
                Err(e) => {
                    attempt += 1;
                    if attempt >= MAX_ATTEMPTS {
                        return Err(io::Error::new(io::ErrorKind::ConnectionAborted, e));
                    }
                    std::thread::sleep(Duration::from_millis(400 * (1 << attempt.min(4))));
                }
            }
        }
    }
}

pub struct HttpRangeReader {
    inner: Arc<Inner>,
    len: u64,
    pos: u64,
    cur: Option<(u64, Vec<u8>)>,
    ahead: Option<(u64, JoinHandle<io::Result<Vec<u8>>>)>,
}

impl HttpRangeReader {
    /// Probe the length with a 1-byte range request.
    pub fn open(url: &str) -> io::Result<Self> {
        let inner = Arc::new(Inner {
            url: url.to_string(),
            agent: crate::agent(),
        });
        let (_, total) = inner.fetch(0, 1)?;
        let len = total.ok_or_else(|| io::Error::other("server did not report length"))?;
        Ok(Self {
            inner,
            len,
            pos: 0,
            cur: None,
            ahead: None,
        })
    }

    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn chunk_len(&self, idx: u64) -> u64 {
        CHUNK.min(self.len.saturating_sub(idx * CHUNK))
    }

    fn load(&mut self, idx: u64) -> io::Result<()> {
        if matches!(&self.cur, Some((i, _)) if *i == idx) {
            return Ok(());
        }
        let data = match self.ahead.take() {
            Some((i, h)) if i == idx => h
                .join()
                .map_err(|_| io::Error::other("prefetch panicked"))??,
            other => {
                drop(other); // abandoned read-ahead finishes in the background
                self.inner.fetch(idx * CHUNK, self.chunk_len(idx))?.0
            }
        };
        self.cur = Some((idx, data));
        let next = idx + 1;
        if next * CHUNK < self.len {
            let inner = self.inner.clone();
            let n = self.chunk_len(next);
            self.ahead = Some((
                next,
                std::thread::spawn(move || inner.fetch(next * CHUNK, n).map(|v| v.0)),
            ));
        }
        Ok(())
    }
}

impl Read for HttpRangeReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len || out.is_empty() {
            return Ok(0);
        }
        let idx = self.pos / CHUNK;
        self.load(idx)?;
        let (_, data) = self.cur.as_ref().unwrap();
        let off = (self.pos - idx * CHUNK) as usize;
        if off >= data.len() {
            return Ok(0);
        }
        let n = out.len().min(data.len() - off);
        out[..n].copy_from_slice(&data[off..off + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for HttpRangeReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let new = match to {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(n) => self.len as i128 + n as i128,
            SeekFrom::Current(n) => self.pos as i128 + n as i128,
        };
        if new < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}
