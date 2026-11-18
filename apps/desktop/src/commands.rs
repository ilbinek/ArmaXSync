//! Tauri commands (thin wrappers around `axs-client` / `axs-launcher`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use axs_client::{AppConfig, Client, RepoConfig, SyncRequest};
use axs_launcher::Profile;
use axs_sync::{CancellationToken, SyncEvent};

use crate::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn client(st: &State<'_, AppState>) -> CmdResult<Client> {
    Client::open(st.dirs.clone()).map_err(err)
}

#[derive(Serialize)]
pub struct GameInfo {
    arma3_dir: Option<PathBuf>,
    steam: bool,
    workshop_dirs: Vec<PathBuf>,
    linux: bool,
}

#[derive(Serialize)]
pub struct AppStateOut {
    config: AppConfig,
    repos: Vec<RepoConfig>,
    profiles: Vec<String>,
    active_profile: String,
    data_dir: PathBuf,
    portable: bool,
    game: GameInfo,
    syncing: Vec<String>,
    version: &'static str,
}

#[tauri::command]
pub fn app_state(st: State<'_, AppState>) -> CmdResult<AppStateOut> {
    let c = client(&st)?;
    let profiles = c.profiles().list().into_iter().map(|p| p.name).collect();
    Ok(AppStateOut {
        active_profile: c.config.profile.clone().unwrap_or_else(|| "Default".into()),
        repos: c.repos().to_vec(),
        config: c.config.clone(),
        profiles,
        data_dir: c.dirs.config.clone(),
        portable: c.dirs.portable,
        game: GameInfo {
            arma3_dir: axs_launcher::detect::arma3_dir(),
            steam: axs_launcher::detect::steam_root().is_some(),
            workshop_dirs: axs_launcher::detect::workshop_dirs(),
            linux: cfg!(target_os = "linux"),
        },
        syncing: st.syncs.lock().unwrap().keys().cloned().collect(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
pub fn save_config(st: State<'_, AppState>, config: AppConfig) -> CmdResult<()> {
    let mut c = client(&st)?;
    c.config = config;
    c.save_config().map_err(err)
}

#[tauri::command]
pub async fn repo_resolve(
    st: State<'_, AppState>,
    url: String,
    login: Option<String>,
    password: Option<String>,
) -> CmdResult<RepoConfig> {
    let c = client(&st)?;
    c.resolve_url(
        &url,
        login.as_deref().filter(|s| !s.is_empty()),
        password.as_deref().filter(|s| !s.is_empty()),
    )
    .await
    .map_err(err)
}

#[tauri::command]
pub fn repo_save(
    st: State<'_, AppState>,
    repo: RepoConfig,
    original: Option<String>,
) -> CmdResult<()> {
    let mut c = client(&st)?;
    match original {
        Some(o) if o != repo.name => {
            c.rename_repo(&o, &repo.name).map_err(err)?;
            c.save_repo(repo).map_err(err)
        }
        Some(_) => c.save_repo(repo).map_err(err),
        None => c.add_repo(repo).map_err(err),
    }
}

#[tauri::command]
pub fn repo_remove(st: State<'_, AppState>, name: String) -> CmdResult<()> {
    client(&st)?.remove_repo(&name).map_err(err)
}

#[tauri::command]
pub async fn repo_check(
    st: State<'_, AppState>,
    name: String,
) -> CmdResult<axs_client::CheckOutcome> {
    client(&st)?.check(&name).await.map_err(err)
}

#[tauri::command]
pub async fn repo_check_all(st: State<'_, AppState>) -> CmdResult<Vec<axs_client::CheckOutcome>> {
    let mut c = client(&st)?;
    let names: Vec<String> = c.repos().iter().map(|r| r.name.clone()).collect();
    let mut out = vec![];
    for n in names {
        out.push(c.check(&n).await.map_err(err)?);
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct RepoInfo {
    revision: u32,
    source: String,
    events: Vec<axs_model::EventDef>,
    changelogs: Vec<axs_model::ChangelogEntry>,
    addons: Vec<String>,
    servers: Vec<axs_model::ServerDef>,
    files: usize,
    size: u64,
}

#[tauri::command]
pub async fn repo_info(st: State<'_, AppState>, name: String) -> CmdResult<RepoInfo> {
    let c = client(&st)?;
    let r = c.repo(&name).map_err(err)?.clone();
    let t = c.transport(&r).map_err(err)?;
    let s = axs_sync::fetch_snapshot(&t, r.prefer_modern)
        .await
        .map_err(err)?;
    Ok(RepoInfo {
        revision: s.manifest.revision,
        source: format!("{:?}", s.source),
        files: s.manifest.files.len(),
        size: s.manifest.total_size(),
        addons: s
            .manifest
            .addons
            .iter()
            .map(|a| axs_model::paths::file_name(a).to_owned())
            .collect(),
        events: s.manifest.events,
        changelogs: s.manifest.changelogs,
        servers: s.manifest.servers,
    })
}

#[derive(Deserialize, Default)]
pub struct SyncParams {
    event: Option<String>,
    exact: Option<bool>,
    destination: Option<PathBuf>,
    no_delete: Option<bool>,
}

impl SyncParams {
    fn request(&self, dry: bool) -> SyncRequest {
        SyncRequest {
            destination: self.destination.clone(),
            event: self.event.clone().filter(|e| !e.is_empty()),
            exact_match: self.exact,
            deep: false,
            dry_run: dry,
            no_delete: self.no_delete.unwrap_or(false),
        }
    }
}

#[tauri::command]
pub async fn sync_plan(
    st: State<'_, AppState>,
    name: String,
    params: Option<SyncParams>,
) -> CmdResult<axs_client::SyncOutcome> {
    let mut c = client(&st)?;
    let p = params.unwrap_or_default();
    c.sync(
        &name,
        &p.request(true),
        Arc::new(|_| {}),
        CancellationToken::new(),
    )
    .await
    .map_err(err)
}

#[derive(Serialize, Clone, Default)]
struct Progress {
    repo: String,
    bytes_total: u64,
    bytes_done: u64,
    files_total: usize,
    files_done: usize,
    failed: usize,
    current: String,
    speed: f64,
}

/// Start a sync in the background; progress via the `sync-progress` and
/// `sync-done` events.
#[tauri::command]
pub async fn sync_start(
    app: AppHandle,
    st: State<'_, AppState>,
    name: String,
    params: Option<SyncParams>,
) -> CmdResult<()> {
    let token = CancellationToken::new();
    {
        let mut s = st.syncs.lock().unwrap();
        if s.contains_key(&name) {
            return Err(format!("{name} is already synchronizing"));
        }
        s.insert(name.clone(), token.clone());
    }
    let dirs = st.dirs.clone();
    let p = params.unwrap_or_default();
    let req = p.request(false);
    tauri::async_runtime::spawn(async move {
        let progress = Arc::new(Mutex::new(Progress {
            repo: name.clone(),
            ..Default::default()
        }));
        let done = Arc::new(AtomicU64::new(0));
        let (p2, d2) = (progress.clone(), done.clone());
        let events: axs_sync::EventFn = Arc::new(move |e| match e {
            SyncEvent::Started { files, bytes, .. } => {
                let mut p = p2.lock().unwrap();
                p.files_total = files;
                p.bytes_total = bytes;
            }
            SyncEvent::Bytes { n } => {
                d2.fetch_add(n, Ordering::Relaxed);
            }
            SyncEvent::FileStarted { path, .. } => p2.lock().unwrap().current = path,
            SyncEvent::FileDone { reused, .. } => {
                d2.fetch_add(reused, Ordering::Relaxed);
                p2.lock().unwrap().files_done += 1;
            }
            SyncEvent::FileFailed { .. } => p2.lock().unwrap().failed += 1,
            _ => {}
        });
        // Throttled progress emitter.
        let (app2, p3, d3, tok2) = (app.clone(), progress.clone(), done.clone(), token.clone());
        let ticker = tauri::async_runtime::spawn(async move {
            let mut last = (std::time::Instant::now(), 0u64);
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let mut p = p3.lock().unwrap().clone();
                p.bytes_done = d3
                    .load(Ordering::Relaxed)
                    .min(p.bytes_total.max(d3.load(Ordering::Relaxed)));
                let dt = last.0.elapsed().as_secs_f64();
                if dt > 0.9 {
                    p.speed = (p.bytes_done.saturating_sub(last.1)) as f64 / dt;
                    last = (std::time::Instant::now(), p.bytes_done);
                }
                let _ = app2.emit("sync-progress", p);
                if tok2.is_cancelled() {
                    break;
                }
            }
        });
        let result = async {
            let mut c = Client::open(dirs).map_err(err)?;
            c.sync(&name, &req, events, token.clone())
                .await
                .map_err(err)
        }
        .await;
        token.cancel();
        let _ = ticker.await;
        app.state::<AppState>().syncs.lock().unwrap().remove(&name);
        let payload = match result {
            Ok(o) => {
                serde_json::json!({"repo": name, "ok": o.report.as_ref().map(|r| r.ok()).unwrap_or(true), "outcome": o})
            }
            Err(e) => serde_json::json!({"repo": name, "ok": false, "error": e}),
        };
        let _ = app.emit("sync-done", payload);
    });
    Ok(())
}

#[tauri::command]
pub fn sync_cancel(st: State<'_, AppState>, name: String) -> CmdResult<()> {
    if let Some(t) = st.syncs.lock().unwrap().get(&name) {
        t.cancel();
    }
    Ok(())
}

#[tauri::command]
pub fn profile_get(st: State<'_, AppState>, name: Option<String>) -> CmdResult<Profile> {
    let c = client(&st)?;
    Ok(match name {
        Some(n) => c.profile(&n),
        None => c.active_profile(),
    })
}

#[tauri::command]
pub fn profile_save(
    st: State<'_, AppState>,
    profile: Profile,
    original: Option<String>,
) -> CmdResult<()> {
    let c = client(&st)?;
    let store = c.profiles();
    if let Some(o) = original.filter(|o| *o != profile.name) {
        if store.get(&profile.name).is_some() {
            return Err(format!("profile {} already exists", profile.name));
        }
        let _ = store.remove(&o);
    }
    store.save(&profile).map_err(err)
}

#[tauri::command]
pub fn profile_remove(st: State<'_, AppState>, name: String) -> CmdResult<()> {
    client(&st)?.profiles().remove(&name).map_err(err)
}

#[tauri::command]
pub fn profile_activate(st: State<'_, AppState>, name: String) -> CmdResult<()> {
    let mut c = client(&st)?;
    c.config.profile = Some(name);
    c.save_config().map_err(err)
}

#[tauri::command]
pub fn addons(st: State<'_, AppState>, profile: Profile) -> CmdResult<Vec<axs_launcher::Addon>> {
    Ok(client(&st)?.addons(&profile).into_values().collect())
}

#[tauri::command]
pub async fn apply_modset(
    st: State<'_, AppState>,
    profile: String,
    repo: String,
    event: Option<String>,
) -> CmdResult<Profile> {
    let mut c = client(&st)?;
    c.apply_modset(&profile, &repo, event.as_deref())
        .await
        .map_err(err)
}

#[derive(Serialize)]
pub struct Preview {
    command: Option<axs_launcher::LaunchCommand>,
    args: Vec<String>,
    error: Option<String>,
    check: axs_launcher::LaunchCheck,
    running: bool,
}

#[tauri::command]
pub fn launch_preview(
    st: State<'_, AppState>,
    profile: Profile,
    server: Option<String>,
) -> CmdResult<Preview> {
    let c = client(&st)?;
    let addons = c.addons(&profile);
    let check = axs_launcher::check(&profile, &addons);
    let srv = server
        .as_deref()
        .or(profile.server.as_deref())
        .filter(|s| !s.is_empty())
        .and_then(|n| c.favorite_server(n));
    let args =
        axs_launcher::run_parameters(&profile, &addons, srv, &axs_launcher::params::native_path);
    let (command, error) = match c.launch_command(&profile, server.as_deref()) {
        Ok(cmd) => (Some(cmd), None),
        Err(e) => (None, Some(err(e))),
    };
    Ok(Preview {
        command,
        args,
        error,
        check,
        running: axs_launcher::detect::arma_running(),
    })
}

#[tauri::command]
pub fn launch_game(
    app: AppHandle,
    st: State<'_, AppState>,
    profile: Profile,
    server: Option<String>,
) -> CmdResult<u32> {
    let c = client(&st)?;
    let cmd = c.launch_command(&profile, server.as_deref()).map_err(err)?;
    let errors = axs_launcher::start_external_apps(&c.config.external_apps);
    for (n, e) in errors {
        tracing::warn!("external application {n}: {e}");
    }
    let pid =
        axs_launcher::launch(&cmd, profile.options.auto_restart, profile.proton).map_err(err)?;
    if let Some(w) = app.get_webview_window("main") {
        match c.config.at_launch {
            axs_client::config::AtLaunch::Minimize => {
                let _ = w.minimize();
            }
            axs_client::config::AtLaunch::Close if !profile.options.auto_restart => app.exit(0),
            _ => {}
        }
    }
    Ok(pid)
}

#[derive(Serialize)]
pub struct GameDetails {
    game_profiles: Vec<String>,
    mallocs: Vec<String>,
    running: bool,
    cpus: usize,
}

#[tauri::command]
pub fn game_info(profile: Profile) -> GameDetails {
    GameDetails {
        game_profiles: axs_launcher::detect::game_profiles(profile.proton),
        mallocs: profile
            .game_dir()
            .map(axs_launcher::detect::mallocs)
            .unwrap_or_default(),
        running: axs_launcher::detect::arma_running(),
        cpus: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
    }
}

#[tauri::command]
pub fn extract_keys(src: PathBuf, dst: PathBuf) -> CmdResult<Vec<String>> {
    axs_launcher::tools::extract_bikeys(&src, &dst).map_err(err)
}

#[tauri::command]
pub fn latest_rpt(proton: bool) -> Option<PathBuf> {
    axs_launcher::detect::latest_rpt(proton)
}

#[derive(Serialize)]
pub struct TextChunk {
    text: String,
    offset: u64,
}

/// Read a text file from `offset` (RPT viewer polling); restarts on truncation.
#[tauri::command]
pub fn read_text_from(path: PathBuf, offset: u64) -> CmdResult<TextChunk> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(&path).map_err(err)?;
    let len = f.metadata().map_err(err)?.len();
    let start = if offset > len { 0 } else { offset };
    // Cap the first read to the last 2 MB.
    let start = if start == 0 && len > 2 << 20 {
        len - (2 << 20)
    } else {
        start
    };
    f.seek(SeekFrom::Start(start)).map_err(err)?;
    let mut buf = vec![];
    f.take(len - start).read_to_end(&mut buf).map_err(err)?;
    Ok(TextChunk {
        text: String::from_utf8_lossy(&buf).into_owned(),
        offset: len,
    })
}

#[derive(Deserialize)]
pub struct ServerHelperParams {
    profile: Profile,
    server_dir: PathBuf,
    target: Option<PathBuf>,
    mods: Vec<String>,
    server_mods: Vec<String>,
    copy: bool,
    lowercase: bool,
}

#[tauri::command]
pub fn server_helper(
    st: State<'_, AppState>,
    params: ServerHelperParams,
) -> CmdResult<axs_launcher::tools::ServerSetup> {
    let c = client(&st)?;
    let addons = c.addons(&params.profile);
    let target = params
        .target
        .clone()
        .unwrap_or_else(|| params.server_dir.join("mods"));
    let mode = if params.copy || params.lowercase {
        axs_launcher::tools::LinkMode::Copy
    } else {
        axs_launcher::tools::LinkMode::Link
    };
    axs_launcher::tools::prepare_server(
        &addons,
        &params.mods,
        &params.server_mods,
        &params.server_dir,
        &target,
        mode,
        params.lowercase,
    )
    .map_err(err)
}

#[tauri::command]
pub fn import_legacy(
    st: State<'_, AppState>,
    path: PathBuf,
    overwrite: bool,
) -> CmdResult<axs_client::legacy_import::FullImportReport> {
    let mut c = client(&st)?;
    axs_client::legacy_import::import_all(&mut c, &path, overwrite).map_err(err)
}

/// Local repository build (admin), progress via `build-progress`.
#[tauri::command]
pub async fn build_repo(
    app: AppHandle,
    st: State<'_, AppState>,
    name: String,
) -> CmdResult<axs_build::BuildReport> {
    let dirs = st.dirs.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut c = Client::open(dirs).map_err(err)?;
        let progress = |e: axs_build::BuildEvent| {
            if let axs_build::BuildEvent::Hashing {
                bytes_done,
                bytes_total,
                ..
            } = e
            {
                let _ = app.emit("build-progress", (bytes_done, bytes_total));
            }
        };
        c.build_admin(&name, &progress, &std::sync::atomic::AtomicBool::new(false))
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// Delta push to an ArmaXSync server (admin settings of the repository).
#[tauri::command]
pub async fn push_repo(
    app: AppHandle,
    st: State<'_, AppState>,
    name: String,
    delete_missing: bool,
) -> CmdResult<axs_client::push::PushReport> {
    let c = client(&st)?;
    let r = c.repo(&name).map_err(err)?.clone();
    let a = r
        .admin
        .clone()
        .ok_or("no admin settings for this repository")?;
    let target = axs_client::push::PushTarget {
        server: a
            .push_server
            .clone()
            .ok_or("set the server URL in the repository's admin settings")?,
        repo: a
            .push_repo
            .clone()
            .ok_or("set the server repository in the admin settings")?,
        token: a
            .push_token
            .clone()
            .ok_or("set an API token in the admin settings")?,
    };
    let path = a
        .path
        .clone()
        .ok_or("set the repository folder in the admin settings")?;
    let opts = axs_client::push::PushOptions {
        delete_missing,
        commit: true,
        ..Default::default()
    };
    let events = Arc::new(move |e: axs_client::push::PushEvent| {
        let msg = match e {
            axs_client::push::PushEvent::Hashing { done, total } => {
                serde_json::json!({"phase":"hashing","done":done,"total":total})
            }
            axs_client::push::PushEvent::Planned { upload_bytes, .. } => {
                serde_json::json!({"phase":"planned","total":upload_bytes})
            }
            axs_client::push::PushEvent::Uploading { path, .. } => {
                serde_json::json!({"phase":"uploading","path":path})
            }
            axs_client::push::PushEvent::Uploaded { bytes } => {
                serde_json::json!({"phase":"uploaded","bytes":bytes})
            }
            axs_client::push::PushEvent::Publishing { job } => {
                serde_json::json!({"phase":"publishing","job":job})
            }
            axs_client::push::PushEvent::JobLog(l) => serde_json::json!({"phase":"log","line":l}),
        };
        let _ = app.emit("push-progress", msg);
    });
    axs_client::push::push(
        &target,
        &path,
        &opts,
        Some(c.dirs.data.join("push-hashcache.zst")),
        events,
    )
    .await
    .map_err(err)
}
