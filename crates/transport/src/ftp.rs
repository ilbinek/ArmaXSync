//! FTP downloads (legacy repositories). Uses the blocking suppaftp client on
//! tokio's blocking pool; chunks are streamed back through a channel.

use std::io::Read;
use std::sync::{Arc, Mutex};

use suppaftp::types::FileType;
use suppaftp::FtpStream;

use crate::{ChunkSink, Endpoint, Fetched, RateLimiter, Result, TransportError};

#[derive(Clone)]
pub struct FtpTransport {
    pub(crate) ep: Endpoint,
    /// Idle control connections for reuse.
    pool: Arc<Mutex<Vec<FtpStream>>>,
}

fn ftp_err(e: impl std::fmt::Display) -> TransportError {
    TransportError::Ftp(e.to_string())
}

impl FtpTransport {
    pub fn new(ep: Endpoint) -> Self {
        Self {
            ep,
            pool: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn path(&self, rel: &str) -> String {
        let base = self.ep.base_path.trim_end_matches('/');
        format!("{base}/{}", rel.trim_start_matches('/'))
    }

    fn connect(ep: &Endpoint) -> Result<FtpStream> {
        use std::net::ToSocketAddrs;
        let addr = (ep.host.as_str(), ep.port)
            .to_socket_addrs()
            .map_err(|e| TransportError::Network(e.to_string()))?
            .next()
            .ok_or_else(|| TransportError::Network(format!("cannot resolve {}", ep.host)))?;
        let mut s = match ep.connect_timeout {
            Some(t) => FtpStream::connect_timeout(addr, t),
            None => FtpStream::connect(addr),
        }
        .map_err(|e| TransportError::Network(e.to_string()))?;
        let login = ep.login.clone().unwrap_or_else(|| "anonymous".into());
        let pass = ep.password.clone().unwrap_or_default();
        s.login(&login, &pass).map_err(|e| {
            if e.to_string().contains("530") {
                TransportError::Unauthorized
            } else {
                ftp_err(e)
            }
        })?;
        s.transfer_type(FileType::Binary).map_err(ftp_err)?;
        if let Some(t) = ep.read_timeout {
            let _ = s.get_ref().set_read_timeout(Some(t));
        }
        Ok(s)
    }

    fn take(&self) -> Result<FtpStream> {
        if let Some(mut s) = self.pool.lock().unwrap().pop() {
            if s.noop().is_ok() {
                return Ok(s);
            }
        }
        Self::connect(&self.ep)
    }

    fn give(&self, s: FtpStream) {
        let mut p = self.pool.lock().unwrap();
        if p.len() < 8 {
            p.push(s);
        }
    }

    pub async fn fetch(
        &self,
        rel: &str,
        start: u64,
        end: Option<u64>,
        sink: &mut dyn ChunkSink,
        limiter: Option<&RateLimiter>,
        on_bytes: &(dyn Fn(u64) + Sync),
    ) -> Result<Fetched> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>>>(8);
        let me = self.clone();
        let path = self.path(rel);
        let rel_owned = rel.to_owned();
        let task = tokio::task::spawn_blocking(move || -> Result<Option<u64>> {
            let mut s = me.take()?;
            let total = s.size(&path).ok().map(|v| v as u64);
            if start > 0 {
                s.resume_transfer(start as usize).map_err(ftp_err)?;
            }
            let mut reader = match s.retr_as_stream(&path) {
                Ok(r) => r,
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("550") {
                        return Err(TransportError::NotFound(rel_owned));
                    }
                    return Err(ftp_err(msg));
                }
            };
            let mut left = end.map(|e| e.saturating_sub(start));
            let mut buf = vec![0u8; 256 * 1024];
            let mut aborted = false;
            loop {
                let cap = match left {
                    Some(0) => {
                        aborted = true;
                        break;
                    }
                    Some(l) => (l as usize).min(buf.len()),
                    None => buf.len(),
                };
                let n = reader
                    .read(&mut buf[..cap])
                    .map_err(|e| TransportError::Network(e.to_string()))?;
                if n == 0 {
                    break;
                }
                if let Some(l) = left.as_mut() {
                    *l -= n as u64;
                }
                if tx.blocking_send(Ok(buf[..n].to_vec())).is_err() {
                    aborted = true;
                    break;
                }
            }
            if aborted {
                // The data connection was cut short; drop the session.
                drop(reader);
                let _ = s.quit();
            } else {
                s.finalize_retr_stream(reader).map_err(ftp_err)?;
                me.give(s);
            }
            Ok(total)
        });
        let mut written = 0u64;
        while let Some(chunk) = rx.recv().await {
            let chunk = chunk?;
            if let Some(l) = limiter {
                l.consume(chunk.len() as u64).await;
            }
            sink.write_chunk(&chunk)?;
            written += chunk.len() as u64;
            on_bytes(chunk.len() as u64);
        }
        let total = task
            .await
            .map_err(|e| TransportError::Ftp(e.to_string()))??;
        Ok(Fetched {
            bytes: written,
            total_len: total,
        })
    }

    pub async fn exists(&self, rel: &str) -> Result<bool> {
        let me = self.clone();
        let path = self.path(rel);
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let mut s = me.take()?;
            let r = s.size(&path).is_ok();
            me.give(s);
            Ok(r)
        })
        .await
        .map_err(|e| TransportError::Ftp(e.to_string()))?
    }
}
