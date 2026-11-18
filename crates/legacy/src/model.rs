//! Rust mirrors of the legacy `fr.soe.a3s.domain.*` classes stored in a
//! repository's `.a3s/` folder.

use serde::{Deserialize, Serialize};

/// Name of the synthetic root directory of a sync tree.
pub const RACINE: &str = "racine";

/// `fr.soe.a3s.domain.repository.SyncTreeDirectory`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SyncDir {
    pub name: String,
    pub mark_as_addon: bool,
    pub updated: bool,
    pub deleted: bool,
    pub hidden: bool,
    pub children: Vec<SyncNode>,
}

/// `fr.soe.a3s.domain.repository.SyncTreeLeaf`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SyncLeaf {
    pub name: String,
    /// Lowercase hex SHA-1, or `"0"` for empty files.
    pub sha1: Option<String>,
    pub size: i64,
    pub compressed_size: i64,
    pub compressed: bool,
    pub updated: bool,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncNode {
    Dir(SyncDir),
    Leaf(SyncLeaf),
}

impl SyncNode {
    pub fn name(&self) -> &str {
        match self {
            SyncNode::Dir(d) => &d.name,
            SyncNode::Leaf(l) => &l.name,
        }
    }
}

impl SyncDir {
    pub fn root() -> Self {
        Self {
            name: RACINE.into(),
            ..Default::default()
        }
    }

    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Sort children the way the legacy builder does: directories first, then
    /// leaves, each group ordered case-insensitively.
    pub fn sort_legacy(&mut self) {
        self.children.sort_by(|a, b| {
            let ka = matches!(a, SyncNode::Leaf(_));
            let kb = matches!(b, SyncNode::Leaf(_));
            ka.cmp(&kb)
                .then_with(|| java_compare_ignore_case(a.name(), b.name()))
        });
        for c in &mut self.children {
            if let SyncNode::Dir(d) = c {
                d.sort_legacy();
            }
        }
    }

    /// Visit every leaf with its `/`-separated path relative to the root.
    pub fn walk_leaves<'a>(&'a self, f: &mut dyn FnMut(&str, &'a SyncLeaf)) {
        fn rec<'a>(d: &'a SyncDir, prefix: &str, f: &mut dyn FnMut(&str, &'a SyncLeaf)) {
            for c in &d.children {
                let p = if prefix.is_empty() {
                    c.name().to_owned()
                } else {
                    format!("{prefix}/{}", c.name())
                };
                match c {
                    SyncNode::Dir(sub) => rec(sub, &p, f),
                    SyncNode::Leaf(l) => f(&p, l),
                }
            }
        }
        rec(self, "", f)
    }

    pub fn count_leaves(&self) -> usize {
        let mut n = 0;
        self.walk_leaves(&mut |_, _| n += 1);
        n
    }
}

/// `String.compareToIgnoreCase` semantics (per UTF-16 unit, upper then lower).
pub fn java_compare_ignore_case(a: &str, b: &str) -> std::cmp::Ordering {
    let fold = |c: u16| -> u16 {
        let ch = char::from_u32(c as u32);
        match ch {
            Some(ch) => {
                let up = ch.to_uppercase().next().unwrap_or(ch);
                let lo = up.to_lowercase().next().unwrap_or(up);
                if (lo as u32) <= 0xFFFF {
                    lo as u16
                } else {
                    c
                }
            }
            None => c,
        }
    };
    let ua: Vec<u16> = a.encode_utf16().collect();
    let ub: Vec<u16> = b.encode_utf16().collect();
    for (x, y) in ua.iter().zip(ub.iter()) {
        if x != y {
            let (fx, fy) = (fold(*x), fold(*y));
            if fx != fy {
                return fx.cmp(&fy);
            }
        }
    }
    ua.len().cmp(&ub.len())
}

/// `fr.soe.a3s.domain.repository.ServerInfo`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub revision: i32,
    /// Milliseconds since the Unix epoch.
    pub build_date: Option<i64>,
    pub number_of_files: i64,
    pub total_files_size: i64,
    pub hidden_folder_paths: Vec<String>,
    pub number_of_connections: i32,
    pub no_partial_file_transfer: bool,
    pub repository_content_updated: bool,
    pub compressed_pbo_files_only: bool,
}

impl Default for ServerInfo {
    fn default() -> Self {
        Self {
            revision: 0,
            build_date: None,
            number_of_files: 0,
            total_files_size: 0,
            hidden_folder_paths: vec![],
            number_of_connections: 1,
            no_partial_file_transfer: false,
            repository_content_updated: false,
            compressed_pbo_files_only: false,
        }
    }
}

/// `fr.soe.a3s.domain.repository.Changelogs`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Changelogs {
    pub list: Vec<Changelog>,
}

/// `fr.soe.a3s.domain.repository.Changelog`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Changelog {
    pub revision: i32,
    pub build_date: Option<i64>,
    pub content_updated: bool,
    pub new_addons: Vec<String>,
    pub updated_addons: Vec<String>,
    pub deleted_addons: Vec<String>,
    pub addons: Vec<String>,
}

/// `fr.soe.a3s.domain.repository.Events`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Events {
    pub list: Vec<Event>,
}

/// `fr.soe.a3s.domain.repository.Event`. Map values: `true` = optional.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Event {
    pub name: Option<String>,
    pub description: Option<String>,
    pub addon_names: Vec<(String, bool)>,
    /// `None` when the stored map was null (older files).
    pub userconfig_folder_names: Option<Vec<(String, bool)>>,
}

/// `fr.soe.a3s.domain.repository.AutoConfig` (published `.a3s/autoconfig`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AutoConfig {
    pub repository_name: Option<String>,
    pub protocole: Option<Protocol>,
    pub favorite_servers: Vec<FavoriteServer>,
}

/// Concrete subclass of `fr.soe.a3s.domain.AbstractProtocole`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolClass {
    Ftp,
    /// Used for both HTTP and HTTPS.
    Http,
    /// Proxy settings only.
    Socks,
}

/// `fr.soe.a3s.constant.ProtocolType`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolType {
    Ftp,
    Http,
    Https,
    A3s,
    Socks4,
    Socks5,
    HttpWebdav,
    HttpsWebdav,
}

impl ProtocolType {
    pub const ALL: [ProtocolType; 8] = [
        Self::Ftp,
        Self::Http,
        Self::Https,
        Self::A3s,
        Self::Socks4,
        Self::Socks5,
        Self::HttpWebdav,
        Self::HttpsWebdav,
    ];

    pub fn java_name(self) -> &'static str {
        match self {
            Self::Ftp => "FTP",
            Self::Http => "HTTP",
            Self::Https => "HTTPS",
            Self::A3s => "A3S",
            Self::Socks4 => "SOCKS4",
            Self::Socks5 => "SOCKS5",
            Self::HttpWebdav => "HTTP_WEBDAV",
            Self::HttpsWebdav => "HTTPS_WEBDAV",
        }
    }

    pub fn from_java_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.java_name() == s)
    }

    /// URL scheme prefix (`"https://"` …).
    pub fn prompt(self) -> &'static str {
        match self {
            Self::Ftp => "ftp://",
            Self::Http | Self::HttpWebdav => "http://",
            Self::Https | Self::HttpsWebdav => "https://",
            Self::A3s => "a3s://",
            Self::Socks4 => "socks4://",
            Self::Socks5 => "socks5://",
        }
    }

    pub fn default_port(self) -> &'static str {
        match self {
            Self::Ftp => "21",
            Self::Http | Self::HttpWebdav => "80",
            Self::Https | Self::HttpsWebdav => "443",
            Self::A3s => "",
            Self::Socks4 | Self::Socks5 => "1080",
        }
    }
}

/// `fr.soe.a3s.constant.EncryptionMode` (deprecated in the legacy app).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionMode {
    NoEncryption,
    ExplicitSsl,
    ImplicitSsl,
}

impl EncryptionMode {
    pub fn java_name(self) -> &'static str {
        match self {
            Self::NoEncryption => "NO_ENCRYPTION",
            Self::ExplicitSsl => "EXPLICIT_SSL",
            Self::ImplicitSsl => "IMPLICIT_SSL",
        }
    }
    pub fn from_java_name(s: &str) -> Option<Self> {
        [Self::NoEncryption, Self::ExplicitSsl, Self::ImplicitSsl]
            .into_iter()
            .find(|e| e.java_name() == s)
    }
}

/// `fr.soe.a3s.domain.AbstractProtocole` and subclasses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Protocol {
    pub class: ProtocolClass,
    pub login: Option<String>,
    pub password: Option<String>,
    /// `host[/path]` without scheme or port.
    pub url: Option<String>,
    pub port: Option<String>,
    pub protocol_type: Option<ProtocolType>,
    pub encryption_mode: Option<EncryptionMode>,
    /// Milliseconds as a decimal string; `"0"` = no timeout, `None` = default.
    pub connection_timeout: Option<String>,
    pub read_timeout: Option<String>,
}

impl Protocol {
    pub fn http(
        protocol_type: ProtocolType,
        url: &str,
        port: &str,
        login: &str,
        password: &str,
    ) -> Self {
        Self {
            class: if protocol_type == ProtocolType::Ftp {
                ProtocolClass::Ftp
            } else {
                ProtocolClass::Http
            },
            login: Some(login.into()),
            password: Some(password.into()),
            url: Some(url.into()),
            port: Some(port.into()),
            protocol_type: Some(protocol_type),
            encryption_mode: None,
            connection_timeout: None,
            read_timeout: None,
        }
    }

    /// Effective protocol type (Java falls back on the class).
    pub fn effective_type(&self) -> ProtocolType {
        self.protocol_type.unwrap_or(match self.class {
            ProtocolClass::Ftp => ProtocolType::Ftp,
            ProtocolClass::Http => ProtocolType::Http,
            ProtocolClass::Socks => ProtocolType::Socks5,
        })
    }

    /// Host part of `url` (text before the first `/`).
    pub fn hostname(&self) -> &str {
        let u = self.url.as_deref().unwrap_or("");
        u.split('/').next().unwrap_or("")
    }

    /// Path part of `url` (from the first `/`, or empty).
    pub fn remote_path(&self) -> &str {
        let u = self.url.as_deref().unwrap_or("");
        u.find('/').map(|i| &u[i..]).unwrap_or("")
    }
}

/// `fr.soe.a3s.domain.configration.FavoriteServer`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FavoriteServer {
    pub name: Option<String>,
    pub ip_address: Option<String>,
    pub port: i32,
    pub password: Option<String>,
    pub selected: bool,
    pub modset_name: Option<String>,
    pub repository_name: Option<String>,
}
