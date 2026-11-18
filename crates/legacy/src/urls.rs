//! Repository URLs in the legacy representation (`AbstractProtocole.url` is
//! `host[/path]`, port stored separately) and auto-config URLs.

use crate::model::{Protocol, ProtocolClass, ProtocolType};

/// Parse `scheme://host[:port][/path]` into a legacy protocol object. The
/// path is the repository root (anything from `/.a3s` on is stripped).
pub fn protocol_from_url(
    url: &str,
    login: Option<&str>,
    password: Option<&str>,
) -> Option<Protocol> {
    let url = url.trim();
    let (pt, rest) = if let Some(r) = strip_prefix_ci(url, "https://") {
        (ProtocolType::Https, r)
    } else if let Some(r) = strip_prefix_ci(url, "http://") {
        (ProtocolType::Http, r)
    } else {
        let r = strip_prefix_ci(url, "ftp://")?;
        (ProtocolType::Ftp, r)
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    // Drop user-info if present (credentials come separately).
    let hostport = hostport.rsplit('@').next().unwrap_or(hostport);
    let (host, port) = split_host_port(hostport);
    if host.is_empty() {
        return None;
    }
    let mut path = path.trim_end_matches('/').to_owned();
    if let Some(i) = path.to_lowercase().find("/.a3s") {
        path.truncate(i);
    }
    let port = port
        .map(str::to_owned)
        .unwrap_or_else(|| pt.default_port().to_owned());
    Some(Protocol {
        class: if pt == ProtocolType::Ftp {
            ProtocolClass::Ftp
        } else {
            ProtocolClass::Http
        },
        login: Some(
            login
                .filter(|l| !l.is_empty())
                .unwrap_or("anonymous")
                .to_owned(),
        ),
        password: Some(password.unwrap_or("").to_owned()),
        url: Some(format!("{host}{path}")),
        port: Some(port),
        protocol_type: Some(pt),
        encryption_mode: None,
        connection_timeout: None,
        read_timeout: None,
    })
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

fn split_host_port(hp: &str) -> (&str, Option<&str>) {
    if let Some(stripped) = hp.strip_prefix('[') {
        // IPv6 literal
        if let Some(end) = stripped.find(']') {
            let host = &hp[..end + 2];
            let rest = &stripped[end + 1..];
            return (host, rest.strip_prefix(':').filter(|p| !p.is_empty()));
        }
    }
    match hp.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => (h, Some(p)),
        _ => (hp, None),
    }
}

/// Base URL of the repository (`scheme://host[:port]/path`).
pub fn base_url(p: &Protocol) -> String {
    let pt = p.effective_type();
    let port = p.port.as_deref().unwrap_or("");
    let host = p.hostname();
    let path = p.remote_path();
    if port.is_empty() || port == pt.default_port() {
        format!("{}{}{}", pt.prompt(), host, path)
    } else {
        format!("{}{}:{}{}", pt.prompt(), host, port, path)
    }
}

/// Public auto-config URL, like `AutoConfigURLAccessMethods` + scheme prefix.
pub fn autoconfig_url(p: &Protocol) -> String {
    format!("{}/.a3s/autoconfig", base_url(p))
}

/// URL of a file inside the repository (unencoded, like jazsync's `URL:`).
pub fn file_url(p: &Protocol, rel: &str) -> String {
    format!("{}/{}", base_url(p), rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format() {
        let p = protocol_from_url(
            "https://mods.example.com/r/main/.a3s/autoconfig",
            None,
            None,
        )
        .unwrap();
        assert_eq!(p.url.as_deref(), Some("mods.example.com/r/main"));
        assert_eq!(p.port.as_deref(), Some("443"));
        assert_eq!(p.login.as_deref(), Some("anonymous"));
        assert_eq!(
            autoconfig_url(&p),
            "https://mods.example.com/r/main/.a3s/autoconfig"
        );

        let p = protocol_from_url("http://10.0.0.1:8080/", Some("u"), Some("p")).unwrap();
        assert_eq!(p.url.as_deref(), Some("10.0.0.1"));
        assert_eq!(p.port.as_deref(), Some("8080"));
        assert_eq!(base_url(&p), "http://10.0.0.1:8080");
        assert_eq!(file_url(&p, "@a/b.pbo"), "http://10.0.0.1:8080/@a/b.pbo");

        let p = protocol_from_url("ftp://ftp.x.org/arma3/repo", None, None).unwrap();
        assert_eq!(p.class, ProtocolClass::Ftp);
        assert_eq!(p.port.as_deref(), Some("21"));
        assert!(protocol_from_url("gopher://x", None, None).is_none());
    }
}
