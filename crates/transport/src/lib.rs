//! Access to repository files over HTTP(S) and FTP.
//!
//! HTTP is fully async (reqwest + rustls). FTP uses a blocking client on the
//! blocking thread pool, streaming chunks back to the async side.

mod ftp;
mod http;
mod limiter;

use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use limiter::RateLimiter;

pub const USER_AGENT: &str = concat!("ArmaXSync/", env!("CARGO_PKG_VERSION"));

/// Maximum size of a metadata file fetched into memory.
pub const MAX_METADATA_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("login or password wrong or missing")]
    Unauthorized,
    #[error("server answered HTTP {status} for {path}")]
    Status { status: u16, path: String },
    #[error("connection failed: {0}")]
    Network(String),
    #[error("FTP: {0}")]
    Ftp(String),
    #[error("response too large")]
    TooLarge,
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid repository url: {0}")]
    BadUrl(String),
}

impl TransportError {
    /// Transient errors worth retrying.
    pub fn is_retryable(&self) -> bool {
        match self {
            TransportError::Network(_) | TransportError::Ftp(_) => true,
            TransportError::Status { status, .. } => {
                *status >= 500 || *status == 408 || *status == 429
            }
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, TransportError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Http,
    Https,
    Ftp,
}

impl Scheme {
    pub fn default_port(self) -> u16 {
        match self {
            Scheme::Http => 80,
            Scheme::Https => 443,
            Scheme::Ftp => 21,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Scheme::Http => "http",
            Scheme::Https => "https",
            Scheme::Ftp => "ftp",
        }
    }
}

/// Where a repository lives and how to authenticate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
    /// Repository root path, `""` or starting with `/`, no trailing slash.
    pub base_path: String,
    pub login: Option<String>,
    pub password: Option<String>,
    pub connect_timeout: Option<Duration>,
    pub read_timeout: Option<Duration>,
    pub accept_invalid_certs: bool,
    /// Proxy URL (`http://`, `https://`, `socks5://` …).
    pub proxy: Option<String>,
}

impl Endpoint {
    /// Parse `scheme://host[:port]/path`.
    pub fn parse(url: &str, login: Option<&str>, password: Option<&str>) -> Result<Self> {
        let p = axs_legacy::urls::protocol_from_url(url, login, password)
            .ok_or_else(|| TransportError::BadUrl(url.to_owned()))?;
        Self::from_protocol(&p)
    }

    /// From a legacy protocol object (autoconfig / old client settings).
    pub fn from_protocol(p: &axs_legacy::Protocol) -> Result<Self> {
        use axs_legacy::ProtocolType as T;
        let scheme = match p.effective_type() {
            T::Http | T::HttpWebdav => Scheme::Http,
            T::Https | T::HttpsWebdav => Scheme::Https,
            T::Ftp => Scheme::Ftp,
            other => {
                return Err(TransportError::BadUrl(format!(
                    "unsupported protocol {other:?}"
                )))
            }
        };
        let host = p.hostname().to_owned();
        if host.is_empty() {
            return Err(TransportError::BadUrl("empty host".into()));
        }
        let port = p
            .port
            .as_deref()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or_else(|| scheme.default_port());
        let ms = |s: &Option<String>| -> Option<Duration> {
            match s.as_deref().map(str::trim) {
                None | Some("") => Some(Duration::from_millis(300_000)),
                Some("0") => None,
                Some(v) => v.parse::<u64>().ok().map(Duration::from_millis),
            }
        };
        Ok(Self {
            scheme,
            host,
            port,
            base_path: p.remote_path().trim_end_matches('/').to_owned(),
            login: p.login.clone().filter(|l| !l.is_empty()),
            password: p.password.clone(),
            connect_timeout: ms(&p.connection_timeout),
            read_timeout: ms(&p.read_timeout),
            accept_invalid_certs: false,
            proxy: None,
        })
    }

    pub fn is_anonymous(&self) -> bool {
        self.login
            .as_deref()
            .map(|l| l.eq_ignore_ascii_case("anonymous"))
            .unwrap_or(true)
    }

    /// `scheme://host[:port]/base` (no credentials).
    pub fn base_url(&self) -> String {
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == self.scheme.default_port() {
            format!("{}://{}{}", self.scheme.as_str(), host, self.base_path)
        } else {
            format!(
                "{}://{}:{}{}",
                self.scheme.as_str(),
                host,
                self.port,
                self.base_path
            )
        }
    }

    /// Full URL of a repository-relative path (percent-encoded).
    pub fn url_for(&self, rel: &str) -> String {
        use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
        const SEG: &AsciiSet = &CONTROLS
            .add(b' ')
            .add(b'"')
            .add(b'#')
            .add(b'%')
            .add(b'<')
            .add(b'>')
            .add(b'?')
            .add(b'`')
            .add(b'{')
            .add(b'}')
            .add(b'[')
            .add(b']')
            .add(b'^')
            .add(b'|')
            .add(b'\\');
        let mut url = self.base_url();
        for seg in rel.split('/').filter(|s| !s.is_empty()) {
            url.push('/');
            url.extend(utf8_percent_encode(seg, SEG));
        }
        url
    }
}

/// Receives downloaded bytes.
pub trait ChunkSink: Send {
    fn write_chunk(&mut self, data: &[u8]) -> std::io::Result<()>;
}

impl ChunkSink for Vec<u8> {
    fn write_chunk(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.extend_from_slice(data);
        Ok(())
    }
}

impl ChunkSink for std::fs::File {
    fn write_chunk(&mut self, data: &[u8]) -> std::io::Result<()> {
        std::io::Write::write_all(self, data)
    }
}

impl<W: std::io::Write + Send> ChunkSink for std::io::BufWriter<W> {
    fn write_chunk(&mut self, data: &[u8]) -> std::io::Result<()> {
        std::io::Write::write_all(self, data)
    }
}

/// Result of a (ranged) download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fetched {
    /// Bytes delivered to the sink.
    pub bytes: u64,
    /// Total length of the remote file, if the server told us.
    pub total_len: Option<u64>,
}

/// A connection to one repository.
#[derive(Clone)]
pub enum Transport {
    Http(http::HttpTransport),
    Ftp(ftp::FtpTransport),
}

impl Transport {
    pub fn new(ep: Endpoint) -> Result<Self> {
        Ok(match ep.scheme {
            Scheme::Http | Scheme::Https => Transport::Http(http::HttpTransport::new(ep)?),
            Scheme::Ftp => Transport::Ftp(ftp::FtpTransport::new(ep)),
        })
    }

    pub fn endpoint(&self) -> &Endpoint {
        match self {
            Transport::Http(h) => &h.ep,
            Transport::Ftp(f) => &f.ep,
        }
    }

    /// Whether byte ranges (resume / delta) can be used.
    pub fn is_http(&self) -> bool {
        matches!(self, Transport::Http(_))
    }

    /// Fetch a small file into memory.
    pub async fn get_bytes(&self, rel: &str) -> Result<Vec<u8>> {
        let mut v = Vec::new();
        let f = self.fetch(rel, 0, None, &mut v, None, &|_| {}).await?;
        if f.bytes > MAX_METADATA_BYTES {
            return Err(TransportError::TooLarge);
        }
        Ok(v)
    }

    /// Download `[start, end)` of a file (`end = None`: to EOF) into `sink`.
    /// `on_bytes` is called with the size of every chunk written.
    pub async fn fetch(
        &self,
        rel: &str,
        start: u64,
        end: Option<u64>,
        sink: &mut dyn ChunkSink,
        limiter: Option<&RateLimiter>,
        on_bytes: &(dyn Fn(u64) + Sync),
    ) -> Result<Fetched> {
        match self {
            Transport::Http(h) => h.fetch(rel, start, end, sink, limiter, on_bytes).await,
            Transport::Ftp(f) => f.fetch(rel, start, end, sink, limiter, on_bytes).await,
        }
    }

    pub async fn exists(&self, rel: &str) -> Result<bool> {
        match self {
            Transport::Http(h) => h.exists(rel).await,
            Transport::Ftp(f) => f.exists(rel).await,
        }
    }

    /// Legacy probe: `Range: bytes=0-1` on a known file must give 206.
    pub async fn supports_ranges(&self, probe_rel: &str) -> bool {
        match self {
            Transport::Http(h) => h.supports_ranges(probe_rel).await,
            Transport::Ftp(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let ep = Endpoint::parse("https://mods.example.com/r/main/", None, None).unwrap();
        assert_eq!(ep.base_url(), "https://mods.example.com/r/main");
        assert_eq!(
            ep.url_for("@Mód ü/addons/a b.pbo"),
            "https://mods.example.com/r/main/@M%C3%B3d%20%C3%BC/addons/a%20b.pbo"
        );
        let ep = Endpoint::parse("http://10.0.0.1:8080", Some("u"), Some("p")).unwrap();
        assert_eq!(ep.url_for(".a3s/sync"), "http://10.0.0.1:8080/.a3s/sync");
        assert!(!ep.is_anonymous());
    }
}
