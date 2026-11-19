//! Server self-update from GitHub releases.
//!
//! The release workflow publishes `server-latest.json` (version, notes and, per
//! target, URL + sha256 + minisign signature). The server checks it
//! periodically and shows new versions in the admin UI.
//!
//! Installing depends on how the server runs:
//! - **systemd** (deb/rpm): the server downloads and verifies the new binary into
//!   `<data_dir>/update/` and drops a `request` file. A root-owned path unit
//!   (`armaxsync-server-update.path`) runs `update-helper`, which re-verifies
//!   the binary with the *installed* executable (`verify-update`), swaps it,
//!   restarts the service and rolls back if it does not come up.
//! - **docker** / **manual**: notification only; the UI shows how to update.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::db::now;
use crate::jobs::JobCtx;
use crate::state::Shared;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Target triple this binary was built for (release asset suffix).
pub const TARGET: &str = env!("ARMAXSYNC_TARGET");
/// GitHub `owner/repo` that publishes releases.
pub const RELEASE_REPO: &str = match option_env!("ARMAXSYNC_RELEASE_REPO") {
    Some(r) => r,
    None => "ilbinek/ArmaXSync",
};
/// minisign public key (base64 of the `.pub` file, the format Tauri uses).
/// Must match the private key in the release workflow's secrets.
pub const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEE0NjZBMTY1ODE3Q0M1NTcKUldSWHhYeUJaYUZtcEoybzNISFlqbFRFc3ZrdEtjUjRwN1NOM1BhL3JTcHNrYWxoMXlYcVA5djQK";
/// Installed by the deb/rpm packages; its presence enables one-click updates.
pub const HELPER_PATH: &str = "/usr/lib/armaxsync/update-helper";

pub fn default_manifest_url() -> String {
    format!("https://github.com/{RELEASE_REPO}/releases/latest/download/server-latest.json")
}

// ------------------------------------------------------------------ config ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateConfig {
    /// Look for new releases.
    pub check: bool,
    /// Hours between checks.
    pub interval_hours: u64,
    /// Release manifest (`server-latest.json`).
    pub manifest_url: String,
    /// Install new releases automatically (systemd installs only).
    pub auto_install: bool,
    /// When automatic installs may happen (cron, server local time).
    pub auto_install_cron: String,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            check: true,
            interval_hours: 6,
            manifest_url: default_manifest_url(),
            auto_install: false,
            auto_install_cron: "0 4 * * *".into(),
        }
    }
}

// ---------------------------------------------------------------- manifest ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub pub_date: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub platforms: std::collections::HashMap<String, Platform>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub url: String,
    pub signature: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
}

// ------------------------------------------------------------------ status ----

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InstallKind {
    Systemd,
    Docker,
    Manual,
}

impl InstallKind {
    pub fn detect() -> Self {
        match std::env::var("ARMAXSYNC_INSTALL_KIND").as_deref() {
            Ok("docker") => return InstallKind::Docker,
            Ok("systemd") => return InstallKind::Systemd,
            Ok("manual") => return InstallKind::Manual,
            _ => {}
        }
        if Path::new("/.dockerenv").exists() {
            return InstallKind::Docker;
        }
        if cfg!(unix)
            && std::env::var_os("INVOCATION_ID").is_some()
            && Path::new(HELPER_PATH).exists()
        {
            return InstallKind::Systemd;
        }
        InstallKind::Manual
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Latest {
    pub version: String,
    pub notes: String,
    pub pub_date: Option<String>,
    /// Release page.
    pub url: String,
    /// A build for this server's target is published.
    pub has_build: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub current: &'static str,
    pub target: &'static str,
    pub install_kind: InstallKind,
    pub enabled: bool,
    pub auto_install: bool,
    /// Last check (ms), its error, and what it found.
    pub checked_at: Option<i64>,
    pub error: Option<String>,
    pub latest: Option<Latest>,
    /// A newer version is available.
    pub available: bool,
    /// This server can install it itself.
    pub can_install: bool,
    /// Result of the last install attempt (written by update-helper).
    pub last_result: Option<serde_json::Value>,
    /// An update is staged and waiting for the helper to install it.
    pub staged: Option<String>,
}

pub struct Updates {
    pub status: ArcSwap<UpdateStatus>,
    pub kind: InstallKind,
    manifest: ArcSwap<Option<Manifest>>,
}

impl Updates {
    pub fn new(cfg: &UpdateConfig) -> Self {
        let kind = InstallKind::detect();
        Self {
            status: ArcSwap::from_pointee(UpdateStatus {
                current: VERSION,
                target: TARGET,
                install_kind: kind,
                enabled: cfg.check,
                auto_install: cfg.auto_install && kind == InstallKind::Systemd,
                checked_at: None,
                error: None,
                latest: None,
                available: false,
                can_install: false,
                last_result: None,
                staged: None,
            }),
            kind,
            manifest: ArcSwap::from_pointee(None),
        }
    }

    /// Current status, with the helper's last result read fresh from disk.
    pub fn snapshot(&self, data_dir: &Path) -> UpdateStatus {
        let mut s = (**self.status.load()).clone();
        let dir = update_dir(data_dir);
        s.last_result = std::fs::read(dir.join("last-result.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        s.staged = std::fs::read_to_string(dir.join("request"))
            .ok()
            .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
            .and_then(|v| v["version"].as_str().map(str::to_owned));
        s
    }
}

pub fn update_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("update")
}

/// `true` when `candidate` is a strictly newer semver than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (
        semver::Version::parse(candidate.trim_start_matches('v')),
        semver::Version::parse(current.trim_start_matches('v')),
    ) {
        (Ok(a), Ok(b)) => a > b,
        _ => false,
    }
}

fn http() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(axs_transport::USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .build()?)
}

pub async fn fetch_manifest(url: &str) -> anyhow::Result<Manifest> {
    let r = http()?
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    anyhow::ensure!(
        r.status().is_success(),
        "release manifest {url}: HTTP {}",
        r.status()
    );
    let b = r.bytes().await?;
    serde_json::from_slice(&b).map_err(|e| anyhow::anyhow!("release manifest {url}: {e}"))
}

/// Check for a new release now and update the shared status.
pub async fn check(state: &Shared) -> anyhow::Result<()> {
    let cfg = &state.cfg.updates;
    let u = &state.updates;
    let res = fetch_manifest(&cfg.manifest_url).await;
    let mut s = (**u.status.load()).clone();
    s.checked_at = Some(now());
    match res {
        Ok(m) => {
            let has_build = m.platforms.contains_key(TARGET);
            s.available = is_newer(&m.version, VERSION);
            s.can_install = s.available && has_build && u.kind == InstallKind::Systemd;
            s.latest = Some(Latest {
                url: format!(
                    "https://github.com/{RELEASE_REPO}/releases/tag/v{}",
                    m.version
                ),
                version: m.version.clone(),
                notes: m.notes.clone(),
                pub_date: m.pub_date.clone(),
                has_build,
            });
            s.error = None;
            if s.available {
                tracing::info!("update available: {} → {}", VERSION, m.version);
            }
            u.manifest.store(Arc::new(Some(m)));
            u.status.store(Arc::new(s));
            Ok(())
        }
        Err(e) => {
            s.error = Some(format!("{e:#}"));
            u.status.store(Arc::new(s));
            Err(e)
        }
    }
}

/// Periodic checks, plus automatic installs in the configured window.
pub fn spawn(state: Shared) {
    let cfg = state.cfg.updates.clone();
    if !cfg.check {
        return;
    }
    tokio::spawn(async move {
        // Let startup settle (and avoid hammering GitHub on restart loops).
        tokio::time::sleep(Duration::from_secs(30)).await;
        let interval = Duration::from_secs(cfg.interval_hours.max(1) * 3600);
        let mut next_check = tokio::time::Instant::now();
        let auto = cfg.auto_install && state.updates.kind == InstallKind::Systemd;
        let mut next_auto = crate::scheduler::next_run(&cfg.auto_install_cron, now());
        if auto && next_auto.is_none() {
            tracing::warn!(
                "updates.auto_install_cron {:?} is not a valid cron expression; automatic updates disabled",
                cfg.auto_install_cron
            );
        }
        loop {
            if tokio::time::Instant::now() >= next_check {
                if let Err(e) = check(&state).await {
                    tracing::warn!("update check: {e:#}");
                }
                next_check = tokio::time::Instant::now() + interval;
            }
            if auto {
                if let Some(t) = next_auto.filter(|t| now() >= *t) {
                    next_auto = crate::scheduler::next_run(&cfg.auto_install_cron, t.max(now()));
                    let st = state.updates.snapshot(&state.cfg.data_dir);
                    if st.can_install && st.staged.is_none() {
                        let v = st.latest.map(|l| l.version).unwrap_or_default();
                        tracing::info!("installing update {v} (automatic)");
                        if let Err(e) = start_install(&state, v, None).await {
                            tracing::warn!("automatic update: {e:#}");
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

/// Queue the self-update job (it waits for other jobs to finish).
pub async fn start_install(
    state: &Shared,
    version: String,
    user_id: Option<i64>,
) -> anyhow::Result<i64> {
    anyhow::ensure!(
        state.updates.kind == InstallKind::Systemd,
        "this server cannot update itself ({:?} install)",
        state.updates.kind
    );
    crate::jobs::JobManager::submit(
        state,
        crate::jobs::JobKind::SelfUpdate { version },
        None,
        user_id,
    )
    .await
}

/// Verify a release binary's minisign signature (base64 `.sig` content).
pub fn verify_signature(data: &[u8], signature_b64: &str) -> anyhow::Result<()> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let pk_text = String::from_utf8(b64.decode(PUBLIC_KEY)?)?;
    let pk = minisign_verify::PublicKey::decode(&pk_text)
        .map_err(|e| anyhow::anyhow!("public key: {e}"))?;
    let sig_text = String::from_utf8(
        b64.decode(signature_b64.trim())
            .map_err(|e| anyhow::anyhow!("signature is not base64: {e}"))?,
    )?;
    let sig = minisign_verify::Signature::decode(&sig_text)
        .map_err(|e| anyhow::anyhow!("signature: {e}"))?;
    pk.verify(data, &sig, false)
        .map_err(|e| anyhow::anyhow!("signature verification failed: {e}"))
}

/// The self-update job: download, verify, stage, request installation.
pub async fn install_job(
    state: &Shared,
    version: &str,
    ctx: &JobCtx,
) -> anyhow::Result<serde_json::Value> {
    // Never swap binaries under a running publish / mirror.
    loop {
        let id = ctx.id;
        let busy: i64 = state
            .db
            .call(move |c| {
                c.query_row(
                    "SELECT COUNT(*) FROM jobs WHERE status='running' AND id<>?1",
                    [id],
                    |r| r.get(0),
                )
            })
            .await?;
        if busy == 0 {
            break;
        }
        ctx.log(format!("waiting for {busy} running job(s) to finish"));
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(10)) => {}
            _ = ctx.cancel.cancelled() => anyhow::bail!("cancelled"),
        }
    }

    ctx.log(format!("checking {}", state.cfg.updates.manifest_url));
    let m = fetch_manifest(&state.cfg.updates.manifest_url).await?;
    anyhow::ensure!(
        m.version == version,
        "the latest release is now {}, not {version}; check for updates again",
        m.version
    );
    anyhow::ensure!(
        is_newer(&m.version, VERSION),
        "{} is not newer than the running version {VERSION}",
        m.version
    );
    let p = m
        .platforms
        .get(TARGET)
        .ok_or_else(|| anyhow::anyhow!("release {} has no build for {TARGET}", m.version))?;

    let dir = update_dir(&state.cfg.data_dir);
    tokio::fs::create_dir_all(&dir).await?;
    let part = dir.join("armaxsync-server.download");
    ctx.log(format!("downloading {}", p.url));
    let data = download(&p.url, p.size, ctx).await?;

    let sha = hex(&sha2::Sha256::digest(&data));
    anyhow::ensure!(
        sha.eq_ignore_ascii_case(&p.sha256),
        "checksum mismatch (expected {}, got {sha})",
        p.sha256
    );
    verify_signature(&data, &p.signature)?;
    ctx.log("checksum and signature verified");

    tokio::fs::write(&part, &data).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755)).await?;
    }
    let new = dir.join("armaxsync-server.new");
    tokio::fs::write(dir.join("armaxsync-server.new.sig"), &p.signature).await?;
    tokio::fs::rename(&part, &new).await?;
    let _ = tokio::fs::remove_file(dir.join("last-result.json")).await;

    // The path unit fires as soon as `request` exists.
    let req = serde_json::json!({
        "version": m.version,
        "from": VERSION,
        "sha256": sha,
        "requested_at": now(),
    });
    tokio::fs::write(dir.join("request.tmp"), req.to_string()).await?;
    tokio::fs::rename(dir.join("request.tmp"), dir.join("request")).await?;
    ctx.log(format!(
        "update {} staged; the server restarts in a few seconds",
        m.version
    ));

    // If nothing picks the request up, say so instead of waiting forever.
    let state2 = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(120)).await;
        let dir = update_dir(&state2.cfg.data_dir);
        if dir.join("request").exists() {
            let _ = std::fs::remove_file(dir.join("request"));
            let msg = "the update helper did not run; is armaxsync-server-update.path enabled? (systemctl enable --now armaxsync-server-update.path)";
            tracing::error!("{msg}");
            let r = serde_json::json!({"status": "failed", "message": msg, "at": now()});
            let _ = std::fs::write(dir.join("last-result.json"), r.to_string());
        }
    });

    Ok(serde_json::json!({"staged": m.version}))
}

async fn download(url: &str, size_hint: u64, ctx: &JobCtx) -> anyhow::Result<Vec<u8>> {
    use futures::StreamExt;
    let r = http()?.get(url).send().await?;
    anyhow::ensure!(
        r.status().is_success(),
        "download {url}: HTTP {}",
        r.status()
    );
    let total = r.content_length().unwrap_or(size_hint);
    let mut out = Vec::with_capacity(total as usize);
    let mut s = r.bytes_stream();
    loop {
        let chunk = tokio::select! {
            c = tokio::time::timeout(Duration::from_secs(60), s.next()) => c.map_err(|_| anyhow::anyhow!("download stalled"))?,
            _ = ctx.cancel.cancelled() => anyhow::bail!("cancelled"),
        };
        let Some(chunk) = chunk else { break };
        out.extend_from_slice(&chunk?);
        anyhow::ensure!(out.len() < 512 << 20, "download too large");
        ctx.progress("download", out.len() as u64, total.max(out.len() as u64));
    }
    if size_hint > 0 {
        anyhow::ensure!(
            out.len() as u64 == size_hint,
            "download incomplete ({} of {size_hint} bytes)",
            out.len()
        );
    }
    Ok(out)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `armaxsync-server verify-update`: run by the root update helper with the
/// *installed* binary, so a compromised service account cannot get an
/// unsigned or older binary installed.
pub fn verify_update_cli(file: &Path, signature: &Path, version: &str) -> anyhow::Result<()> {
    let data = std::fs::read(file)?;
    let sig = std::fs::read_to_string(signature)?;
    verify_signature(&data, &sig)?;
    anyhow::ensure!(
        is_newer(version, VERSION),
        "{version} is not newer than the installed version {VERSION}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOB: &[u8] = include_bytes!("../tests/fixtures/update/blob.bin");
    const SIG: &str = include_str!("../tests/fixtures/update/blob.bin.sig");

    #[test]
    fn signature_ok() {
        verify_signature(BLOB, SIG).unwrap();
    }

    #[test]
    fn signature_rejects_tampered() {
        let mut b = BLOB.to_vec();
        b[0] ^= 1;
        assert!(verify_signature(&b, SIG).is_err());
        assert!(verify_signature(BLOB, "bm90IGEgc2lnbmF0dXJl").is_err());
    }

    #[test]
    fn versions() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("0.2.0", "0.2.0-rc.1"));
        assert!(!is_newer("0.2.0-rc.1", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }
}
