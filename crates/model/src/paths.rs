//! Repository-relative path handling shared by builder, sync engine and server.
//!
//! All repository paths are `/`-separated, relative, and never contain `.`/`..`
//! segments, empty segments, NUL or backslashes.

/// Validate and normalize a relative repository path coming from untrusted
/// input (remote manifests, HTTP requests, uploads). Collapses duplicate
/// slashes, strips leading/trailing slashes and rejects traversal.
pub fn sanitize_rel_path(p: &str) -> Option<String> {
    if p.contains('\0') || p.contains('\\') {
        return None;
    }
    let mut out = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" => continue,
            "." | ".." => return None,
            s => {
                // Reject Windows drive prefixes and reserved characters that
                // would let a path escape or misbehave on Windows.
                if s.contains(':')
                    || s.chars().any(|c| {
                        matches!(c, '<' | '>' | '"' | '|' | '?' | '*') || (c as u32) < 0x20
                    })
                {
                    return None;
                }
                if s.ends_with(' ') || s.ends_with('.') {
                    // Windows silently strips these, which can alias files.
                    return None;
                }
                out.push(s);
            }
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out.join("/"))
}

/// Join a sanitized relative path onto a root directory.
pub fn join_rel(root: &std::path::Path, rel: &str) -> std::path::PathBuf {
    let mut p = root.to_path_buf();
    for seg in rel.split('/') {
        if !seg.is_empty() {
            p.push(seg);
        }
    }
    p
}

/// Whether `path` equals `prefix` or lies below it (case-insensitive when
/// `ci`).
pub fn is_under(path: &str, prefix: &str, ci: bool) -> bool {
    let prefix = prefix.trim_matches('/');
    if prefix.is_empty() {
        return true;
    }
    let (p, q) = if ci {
        (path.to_lowercase(), prefix.to_lowercase())
    } else {
        (path.to_owned(), prefix.to_owned())
    };
    p == q || p.starts_with(&format!("{q}/"))
}

/// Parent directory of a relative path (`""` for top-level entries).
pub fn parent(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[..i]).unwrap_or("")
}

/// Last path segment.
pub fn file_name(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[i + 1..]).unwrap_or(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_rel_path("//a//b/").as_deref(), Some("a/b"));
        assert_eq!(
            sanitize_rel_path("@ace/addons/ace.pbo").as_deref(),
            Some("@ace/addons/ace.pbo")
        );
        assert_eq!(sanitize_rel_path("Mód 🙂/x").as_deref(), Some("Mód 🙂/x"));
        for bad in [
            "../x", "a/../b", "a\\b", "c:/x", "a/./b", "", "/", "a\0b", "con. ", "a/b ",
        ] {
            assert!(sanitize_rel_path(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn under() {
        assert!(is_under("userconfig/ace/x", "userconfig", false));
        assert!(is_under("UserConfig/ace", "userconfig/", true));
        assert!(!is_under("userconfig2/x", "userconfig", false));
        assert!(is_under("a", "a", false));
    }
}
