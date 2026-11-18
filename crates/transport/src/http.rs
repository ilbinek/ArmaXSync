use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_RANGE, RANGE};
use reqwest::StatusCode;

use crate::{ChunkSink, Endpoint, Fetched, RateLimiter, Result, TransportError, USER_AGENT};

#[derive(Clone)]
pub struct HttpTransport {
    pub(crate) ep: Endpoint,
    client: reqwest::Client,
}

fn net(e: reqwest::Error) -> TransportError {
    let mut msg = e.to_string();
    let mut src: Option<&dyn std::error::Error> = std::error::Error::source(&e);
    while let Some(s) = src {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        src = s.source();
    }
    TransportError::Network(msg)
}

impl HttpTransport {
    pub fn new(ep: Endpoint) -> Result<Self> {
        let mut b = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .danger_accept_invalid_certs(ep.accept_invalid_certs)
            .pool_max_idle_per_host(16);
        if let Some(t) = ep.connect_timeout {
            b = b.connect_timeout(t);
        }
        if let Some(t) = ep.read_timeout {
            b = b.read_timeout(t);
        }
        if let Some(p) = &ep.proxy {
            b = b.proxy(reqwest::Proxy::all(p).map_err(|e| TransportError::BadUrl(e.to_string()))?);
        }
        let client = b.build().map_err(net)?;
        Ok(Self { ep, client })
    }

    fn request(&self, method: reqwest::Method, rel: &str) -> reqwest::RequestBuilder {
        let mut rb = self.client.request(method, self.ep.url_for(rel));
        if !self.ep.is_anonymous() {
            rb = rb.basic_auth(
                self.ep.login.clone().unwrap_or_default(),
                self.ep.password.clone(),
            );
        }
        rb
    }

    fn check(status: StatusCode, rel: &str) -> Result<()> {
        match status {
            s if s.is_success() => Ok(()),
            StatusCode::NOT_FOUND | StatusCode::GONE => {
                Err(TransportError::NotFound(rel.to_owned()))
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(TransportError::Unauthorized),
            s => Err(TransportError::Status {
                status: s.as_u16(),
                path: rel.to_owned(),
            }),
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
        if let Some(e) = end {
            if e <= start {
                return Ok(Fetched {
                    bytes: 0,
                    total_len: None,
                });
            }
        }
        let mut headers = HeaderMap::new();
        let ranged = start > 0 || end.is_some();
        if ranged {
            let v = match end {
                Some(e) => format!("bytes={}-{}", start, e - 1),
                None => format!("bytes={start}-"),
            };
            headers.insert(RANGE, HeaderValue::from_str(&v).expect("ascii"));
        }
        let resp = self
            .request(reqwest::Method::GET, rel)
            .headers(headers)
            .send()
            .await
            .map_err(net)?;
        let status = resp.status();
        if status == StatusCode::RANGE_NOT_SATISFIABLE {
            return Ok(Fetched {
                bytes: 0,
                total_len: Some(start),
            });
        }
        Self::check(status, rel)?;
        // When the server ignores the Range header (200), skip the prefix.
        let mut skip = if ranged && status == StatusCode::OK {
            start
        } else {
            0
        };
        let total_len = if status == StatusCode::PARTIAL_CONTENT {
            resp.headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit('/').next())
                .and_then(|t| t.parse().ok())
        } else {
            resp.content_length()
        };
        let mut want = end.map(|e| e - start);
        let mut stream = resp.bytes_stream();
        let mut written = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(net)?;
            let mut data = &chunk[..];
            if skip > 0 {
                let s = (skip as usize).min(data.len());
                data = &data[s..];
                skip -= s as u64;
            }
            if let Some(w) = want.as_mut() {
                if (data.len() as u64) > *w {
                    data = &data[..*w as usize];
                }
                *w -= data.len() as u64;
            }
            if data.is_empty() {
                if want == Some(0) {
                    break;
                }
                continue;
            }
            if let Some(l) = limiter {
                l.consume(data.len() as u64).await;
            }
            sink.write_chunk(data)?;
            written += data.len() as u64;
            on_bytes(data.len() as u64);
            if want == Some(0) {
                break;
            }
        }
        Ok(Fetched {
            bytes: written,
            total_len,
        })
    }

    pub async fn exists(&self, rel: &str) -> Result<bool> {
        let resp = self
            .request(reqwest::Method::HEAD, rel)
            .send()
            .await
            .map_err(net)?;
        match Self::check(resp.status(), rel) {
            Ok(()) => Ok(true),
            Err(TransportError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn supports_ranges(&self, probe_rel: &str) -> bool {
        match self
            .request(reqwest::Method::GET, probe_rel)
            .header(RANGE, "bytes=0-1")
            .send()
            .await
        {
            Ok(r) => r.status() == StatusCode::PARTIAL_CONTENT,
            Err(_) => false,
        }
    }
}
