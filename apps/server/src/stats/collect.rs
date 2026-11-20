//! In-memory traffic aggregation. Requests update counters here (one short
//! lock per request, and per few MB while streaming); the 60-second flush
//! moves them to SQLite. Nothing is written to the database per request and
//! no client address is stored: players are counted with a monthly-salted
//! hash folded into HyperLogLog sketches.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use arc_swap::ArcSwap;

use super::hll::Hll;
use crate::db::now;

/// Counter columns, in storage order (also the SQL column names).
pub const COLS: &[&str] = &[
    "bytes",         // content bytes (not metadata, not .zsync/.zip sidecars)
    "bytes_meta",    // .a3s / .axs metadata served from memory
    "bytes_sidecar", // .zsync and .pbo.zip files
    "requests",
    "files",     // completed full-file downloads
    "ranges",    // partial (Range) requests
    "heads",     // HEAD requests
    "checks",    // update checks (serverinfo / index.json fetches)
    "s2xx",      // 200
    "s206",      // 206 Partial Content
    "s3xx",      // redirects
    "s401",      // login required / wrong
    "s404",      // not found
    "s416",      // range not satisfiable
    "s4xx",      // other client errors
    "s503",      // no free download slot
    "s5xx",      // server errors
    "completed", // content transfers that finished
    "aborted",   // content transfers the client dropped
    "xfer_ms",   // total transfer time of finished/aborted transfers
    "wait_ms",   // time waiting for a download slot
    "c_axs",     // requests from ArmaXSync clients
    "c_a3s",     // requests from ArmA3Sync (legacy Java) clients
    "c_browser",
    "c_other",
];
pub const N: usize = 25;

#[allow(non_upper_case_globals)]
pub mod col {
    pub const bytes: usize = 0;
    pub const bytes_meta: usize = 1;
    pub const bytes_sidecar: usize = 2;
    pub const requests: usize = 3;
    pub const files: usize = 4;
    pub const ranges: usize = 5;
    pub const heads: usize = 6;
    pub const checks: usize = 7;
    pub const s2xx: usize = 8;
    pub const s206: usize = 9;
    pub const s3xx: usize = 10;
    pub const s401: usize = 11;
    pub const s404: usize = 12;
    pub const s416: usize = 13;
    pub const s4xx: usize = 14;
    pub const s503: usize = 15;
    pub const s5xx: usize = 16;
    pub const completed: usize = 17;
    pub const aborted: usize = 18;
    pub const xfer_ms: usize = 19;
    pub const wait_ms: usize = 20;
    pub const c_axs: usize = 21;
    pub const c_a3s: usize = 22;
    pub const c_browser: usize = 23;
    pub const c_other: usize = 24;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters(pub [u64; N]);

impl Counters {
    pub fn add(&mut self, o: &Counters) {
        for (a, b) in self.0.iter_mut().zip(o.0.iter()) {
            *a += b;
        }
    }
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|v| *v == 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    ArmaXSync,
    ArmA3Sync,
    Browser,
    Other,
}

impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Family::ArmaXSync => "armaxsync",
            Family::ArmA3Sync => "arma3sync",
            Family::Browser => "browser",
            Family::Other => "other",
        }
    }
    fn col(self) -> usize {
        match self {
            Family::ArmaXSync => col::c_axs,
            Family::ArmA3Sync => col::c_a3s,
            Family::Browser => col::c_browser,
            Family::Other => col::c_other,
        }
    }
}

/// Client family and (for ArmaXSync) version from the User-Agent.
pub fn classify(ua: Option<&str>) -> (Family, Option<String>) {
    let Some(ua) = ua.map(str::trim) else {
        return (Family::Other, None);
    };
    if let Some(v) = ua.strip_prefix("ArmaXSync/") {
        let v: String = v
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
            .take(24)
            .collect();
        return (Family::ArmaXSync, (!v.is_empty()).then_some(v));
    }
    // The Java client sends exactly "ArmA3Sync"; Java's default is "Java/…".
    if ua.eq_ignore_ascii_case("ArmA3Sync") || ua.starts_with("Java/") {
        return (Family::ArmA3Sync, None);
    }
    if ua.starts_with("Mozilla/") {
        return (Family::Browser, None);
    }
    (Family::Other, None)
}

/// Monthly salt for player hashes (the previous month's is deleted).
#[derive(Clone)]
pub struct Salt {
    pub month: String,
    pub key: [u8; 32],
}

pub fn month_of(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap_or_default()
        .format("%Y-%m")
        .to_string()
}

pub fn day_of(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap_or_default()
        .format("%Y-%m-%d")
        .to_string()
}

pub fn hour_of(ms: i64) -> i64 {
    ms.div_euclid(3_600_000)
}

/// Aggregates waiting for the next flush.
#[derive(Default)]
pub struct Acc {
    /// (UTC epoch hour, repo id) → counters
    pub hours: HashMap<(i64, i64), Counters>,
    /// (day, repo id, addon) → (bytes, completed full downloads)
    pub addons: HashMap<(String, i64, String), (u64, u64)>,
    /// (day, family, version) → requests
    pub versions: HashMap<(String, &'static str, String), u64>,
    /// (day, repo id; 0 = whole server) → player sketch
    pub players: HashMap<(String, i64), Hll>,
}

const LIVE_STEP_MS: i64 = 5_000;
const LIVE_STEPS: usize = 180; // 15 minutes
const ACTIVE_WINDOW_MS: i64 = 5 * 60_000;

pub struct Collector {
    pub players_enabled: bool,
    acc: Mutex<Acc>,
    pub salt: ArcSwap<Salt>,
    // Live view (memory only).
    pub live_bytes: AtomicU64,
    pub active_downloads: AtomicI64,
    seen: Mutex<HashMap<u64, i64>>,
    ring: Mutex<VecDeque<(i64, u64)>>,
    last_sample: Mutex<(i64, u64)>,
    /// Since-start totals per family (Prometheus).
    pub family_requests: [AtomicU64; 4],
    pub aborted_total: AtomicU64,
}

impl Collector {
    pub fn new(players_enabled: bool, salt: Salt) -> Self {
        Self {
            players_enabled,
            acc: Mutex::new(Acc::default()),
            salt: ArcSwap::from_pointee(salt),
            live_bytes: AtomicU64::new(0),
            active_downloads: AtomicI64::new(0),
            seen: Mutex::new(HashMap::new()),
            ring: Mutex::new(VecDeque::with_capacity(LIVE_STEPS)),
            last_sample: Mutex::new((now(), 0)),
            family_requests: Default::default(),
            aborted_total: AtomicU64::new(0),
        }
    }

    /// Take everything collected since the last flush.
    pub fn take(&self) -> Acc {
        std::mem::take(&mut *self.acc.lock().unwrap())
    }

    /// Put back aggregates whose flush failed (merged with newer data).
    pub fn restore(&self, old: Acc) {
        let mut a = self.acc.lock().unwrap();
        for (k, v) in old.hours {
            a.hours.entry(k).or_default().add(&v);
        }
        for (k, (b, d)) in old.addons {
            let e = a.addons.entry(k).or_default();
            e.0 += b;
            e.1 += d;
        }
        for (k, v) in old.versions {
            *a.versions.entry(k).or_default() += v;
        }
        for (k, v) in old.players {
            a.players.entry(k).or_default().merge(&v);
        }
    }

    pub fn player_hash(&self, ip: IpAddr) -> u64 {
        let salt = self.salt.load();
        let bytes = match ip {
            IpAddr::V4(v) => v.octets().to_vec(),
            // Normalise IPv4-mapped IPv6 so dual-stack clients count once.
            IpAddr::V6(v) => match v.to_ipv4_mapped() {
                Some(v4) => v4.octets().to_vec(),
                None => v.octets().to_vec(),
            },
        };
        let h = blake3::keyed_hash(&salt.key, &bytes);
        u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap())
    }

    /// Sample cumulative bytes into the live ring buffer (called every 5 s).
    pub fn sample(&self, total_bytes: u64) {
        let t = now();
        let mut last = self.last_sample.lock().unwrap();
        let delta = total_bytes.saturating_sub(last.1);
        *last = (t, total_bytes);
        let mut r = self.ring.lock().unwrap();
        if r.len() == LIVE_STEPS {
            r.pop_front();
        }
        r.push_back((t, delta));
        drop(r);
        self.seen
            .lock()
            .unwrap()
            .retain(|_, at| t - *at < ACTIVE_WINDOW_MS);
    }

    /// (timestamp ms, bytes per second) for the last 15 minutes.
    pub fn live_series(&self) -> Vec<(i64, f64)> {
        self.ring
            .lock()
            .unwrap()
            .iter()
            .map(|(t, b)| (*t, *b as f64 / (LIVE_STEP_MS as f64 / 1000.0)))
            .collect()
    }

    pub fn active_players(&self) -> usize {
        let t = now();
        self.seen
            .lock()
            .unwrap()
            .values()
            .filter(|at| t - **at < ACTIVE_WINDOW_MS)
            .count()
    }

    #[cfg(test)]
    pub fn peek(&self) -> std::sync::MutexGuard<'_, Acc> {
        self.acc.lock().unwrap()
    }
}

/// What kind of file a request touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Meta,
    Content,
    Sidecar,
    None,
}

pub fn is_sidecar(rel: &str) -> bool {
    let l = rel.to_ascii_lowercase();
    l.ends_with(".zsync") || l.ends_with(".pbo.zip")
}

pub fn is_check(rel: &str) -> bool {
    let l = rel.to_ascii_lowercase();
    l == ".a3s/serverinfo" || l == ".axs/index.json"
}

/// Addon a repository path belongs to: the longest published addon folder
/// that contains it, else a top-level `@folder`.
pub fn addon_of(rel: &str, addons: &[String]) -> Option<String> {
    let l = rel.to_ascii_lowercase();
    for a in addons {
        if l.len() > a.len() && l.starts_with(a.as_str()) && l.as_bytes()[a.len()] == b'/' {
            let name_start = a.rfind('/').map(|i| i + 1).unwrap_or(0);
            return Some(rel[name_start..a.len()].to_owned());
        }
    }
    let top = rel.split('/').next()?;
    (top.starts_with('@') && rel.len() > top.len()).then(|| top.to_owned())
}

/// One request being served; records its counters when finished.
pub struct Req {
    stats: Arc<Collector>,
    repo: i64,
    family: Family,
    version: Option<String>,
    ip: Option<IpAddr>,
    pub kind: Kind,
    pub head: bool,
    pub range: bool,
    pub check: bool,
    pub meta_bytes: u64,
}

impl Req {
    pub fn new(
        stats: Arc<Collector>,
        repo: i64,
        ua: Option<&str>,
        ip: Option<IpAddr>,
        head: bool,
    ) -> Self {
        let (family, version) = classify(ua);
        Self {
            stats,
            repo,
            family,
            version,
            ip,
            kind: Kind::None,
            head,
            range: false,
            check: false,
            meta_bytes: 0,
        }
    }

    pub fn family(&self) -> Family {
        self.family
    }

    /// Record the response status and request-level counters.
    pub fn finish(self, status: u16) {
        let s = &self.stats;
        s.family_requests[self.family.col() - col::c_axs].fetch_add(1, Ordering::Relaxed);
        if self.meta_bytes > 0 {
            s.live_bytes.fetch_add(self.meta_bytes, Ordering::Relaxed);
        }
        let t = now();
        let day = day_of(t);
        let player = match (s.players_enabled, self.ip) {
            (true, Some(ip)) => Some(s.player_hash(ip)),
            _ => None,
        };
        if let Some(p) = player {
            s.seen.lock().unwrap().insert(p, t);
        }
        let mut a = s.acc.lock().unwrap();
        let c = a.hours.entry((hour_of(t), self.repo)).or_default();
        let v = &mut c.0;
        v[col::requests] += 1;
        v[self.family.col()] += 1;
        v[col::bytes_meta] += self.meta_bytes;
        if self.head {
            v[col::heads] += 1;
        }
        if self.range {
            v[col::ranges] += 1;
        }
        if self.check {
            v[col::checks] += 1;
        }
        let sc = match status {
            200..=205 | 207..=299 => col::s2xx,
            206 => col::s206,
            300..=399 => col::s3xx,
            401 => col::s401,
            404 => col::s404,
            416 => col::s416,
            400..=499 => col::s4xx,
            503 => col::s503,
            _ => col::s5xx,
        };
        v[sc] += 1;
        if let (Family::ArmaXSync, Some(ver)) = (self.family, &self.version) {
            *a.versions
                .entry((day.clone(), self.family.name(), ver.clone()))
                .or_default() += 1;
        } else if self.family == Family::ArmA3Sync {
            *a.versions
                .entry((day.clone(), self.family.name(), String::new()))
                .or_default() += 1;
        }
        // Only real players: browsers looking at the page are not counted.
        if let Some(p) = player.filter(|_| {
            matches!(
                self.family,
                Family::ArmaXSync | Family::ArmA3Sync | Family::Other
            )
        }) {
            a.players
                .entry((day.clone(), self.repo))
                .or_default()
                .insert(p);
            a.players.entry((day, 0)).or_default().insert(p);
        }
    }
}

/// A content transfer; counts bytes while streaming and the outcome when
/// the body finishes or the client goes away.
pub struct Download {
    stats: Arc<Collector>,
    repo: i64,
    addon: Option<String>,
    sidecar: bool,
    full: bool,
    planned: u64,
    sent: u64,
    pending: u64,
    started: Instant,
    wait_ms: u64,
}

const PENDING_FLUSH: u64 = 4 << 20;

impl Download {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        stats: Arc<Collector>,
        repo: i64,
        addon: Option<String>,
        sidecar: bool,
        full: bool,
        planned: u64,
        wait_ms: u64,
    ) -> Self {
        stats.active_downloads.fetch_add(1, Ordering::Relaxed);
        Self {
            stats,
            repo,
            addon,
            sidecar,
            full,
            planned,
            sent: 0,
            pending: 0,
            started: Instant::now(),
            wait_ms,
        }
    }

    pub fn add(&mut self, n: u64) {
        self.sent += n;
        self.pending += n;
        self.stats.live_bytes.fetch_add(n, Ordering::Relaxed);
        if self.pending >= PENDING_FLUSH {
            self.push(false);
        }
    }

    fn push(&mut self, done: bool) {
        let bytes = std::mem::take(&mut self.pending);
        let completed = done && self.sent >= self.planned;
        let t = now();
        let mut a = self.stats.acc.lock().unwrap();
        let v = &mut a.hours.entry((hour_of(t), self.repo)).or_default().0;
        v[if self.sidecar {
            col::bytes_sidecar
        } else {
            col::bytes
        }] += bytes;
        if done {
            v[if completed {
                col::completed
            } else {
                col::aborted
            }] += 1;
            v[col::xfer_ms] += self.started.elapsed().as_millis() as u64;
            v[col::wait_ms] += self.wait_ms;
            if completed && self.full && !self.sidecar {
                v[col::files] += 1;
            }
        }
        if let Some(addon) = &self.addon {
            if bytes > 0 || done {
                let e = a
                    .addons
                    .entry((day_of(t), self.repo, addon.clone()))
                    .or_default();
                e.0 += bytes;
                if completed && self.full && !self.sidecar {
                    e.1 += 1;
                }
            }
        }
        if done && !completed {
            self.stats.aborted_total.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        self.push(true);
        self.stats.active_downloads.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salt() -> Salt {
        Salt {
            month: "2026-11".into(),
            key: [7; 32],
        }
    }

    #[test]
    fn user_agents() {
        assert_eq!(
            classify(Some("ArmaXSync/0.2.0")),
            (Family::ArmaXSync, Some("0.2.0".into()))
        );
        assert_eq!(
            classify(Some("ArmaXSync/0.2.0-rc.1 (Windows)")),
            (Family::ArmaXSync, Some("0.2.0-rc.1".into()))
        );
        assert_eq!(classify(Some("ArmA3Sync")).0, Family::ArmA3Sync);
        assert_eq!(classify(Some("Java/1.8.0_401")).0, Family::ArmA3Sync);
        assert_eq!(classify(Some("Mozilla/5.0 (X11)")).0, Family::Browser);
        assert_eq!(classify(Some("curl/8")).0, Family::Other);
        assert_eq!(classify(None).0, Family::Other);
    }

    #[test]
    fn addons() {
        let list = vec!["@cup/@cup_terrains".to_owned(), "@ace".to_owned()];
        assert_eq!(
            addon_of("@ace/addons/x.pbo", &list).as_deref(),
            Some("@ace")
        );
        assert_eq!(
            addon_of("@CUP/@cup_terrains/addons/a.pbo", &list).as_deref(),
            Some("@cup_terrains")
        );
        assert_eq!(
            addon_of("@other/addons/x.pbo", &list).as_deref(),
            Some("@other")
        );
        assert_eq!(addon_of("keys/a.bikey", &list), None);
        assert_eq!(addon_of("@ace", &list), None);
    }

    #[test]
    fn requests_and_downloads() {
        let c = Arc::new(Collector::new(true, salt()));
        let ip: IpAddr = "203.0.113.5".parse().unwrap();
        let mut r = Req::new(c.clone(), 1, Some("ArmaXSync/0.2.0"), Some(ip), false);
        r.check = true;
        r.meta_bytes = 100;
        r.finish(200);
        let mut r = Req::new(
            c.clone(),
            1,
            Some("ArmA3Sync"),
            Some("::ffff:203.0.113.5".parse().unwrap()),
            false,
        );
        r.range = true;
        r.finish(206);
        Req::new(
            c.clone(),
            1,
            Some("Mozilla/5.0"),
            Some("198.51.100.1".parse().unwrap()),
            false,
        )
        .finish(404);
        {
            let mut d = Download::new(c.clone(), 1, Some("@ace".into()), false, true, 10, 3);
            d.add(10);
        }
        {
            let mut d = Download::new(c.clone(), 1, Some("@ace".into()), false, true, 10, 0);
            d.add(4); // client went away
        }
        assert_eq!(c.active_downloads.load(Ordering::Relaxed), 0);
        let a = c.peek();
        let v = a.hours.values().next().unwrap().0;
        assert_eq!(v[col::requests], 3);
        assert_eq!(v[col::checks], 1);
        assert_eq!(v[col::ranges], 1);
        assert_eq!(v[col::bytes_meta], 100);
        assert_eq!(v[col::bytes], 14);
        assert_eq!(
            (v[col::completed], v[col::aborted], v[col::files]),
            (1, 1, 1)
        );
        assert_eq!(v[col::wait_ms], 3);
        assert_eq!((v[col::s2xx], v[col::s206], v[col::s404]), (1, 1, 1));
        assert_eq!((v[col::c_axs], v[col::c_a3s], v[col::c_browser]), (1, 1, 1));
        // Same address over IPv4 and IPv4-mapped IPv6 = one player; browsers don't count.
        let day = day_of(now());
        assert_eq!(a.players[&(day.clone(), 1)].count(), 1);
        assert_eq!(a.players[&(day.clone(), 0)].count(), 1);
        assert_eq!(a.addons[&(day, 1, "@ace".into())], (14, 1));
        assert_eq!(a.versions.len(), 2);
    }

    #[test]
    fn players_off_and_salt_changes_hash() {
        let c = Arc::new(Collector::new(false, salt()));
        Req::new(
            c.clone(),
            1,
            Some("ArmaXSync/1"),
            Some("203.0.113.5".parse().unwrap()),
            false,
        )
        .finish(200);
        assert!(c.peek().players.is_empty());
        let ip: IpAddr = "203.0.113.5".parse().unwrap();
        let h1 = c.player_hash(ip);
        c.salt.store(Arc::new(Salt {
            month: "2026-12".into(),
            key: [8; 32],
        }));
        assert_ne!(h1, c.player_hash(ip));
    }
}
