// Fake Tauri backend for UI development in a plain browser:
//   VITE_MOCK=1 pnpm -C ui dev:desktop   → http://localhost:1420
// State lives in memory; reload resets it. Add ?empty to start as a new user.

import type { AppStateOut, Profile, RepoConfig, RepoInfo, Addon, SyncProgress, GroupNode } from "../api";

type Listener = (p: any) => void;
const listeners = new Map<string, Set<Listener>>();
function emit(ev: string, p: any) {
  listeners.get(ev)?.forEach((f) => f(p));
}
export async function listen(ev: string, f: Listener) {
  if (!listeners.has(ev)) listeners.set(ev, new Set());
  listeners.get(ev)!.add(f);
  return () => listeners.get(ev)?.delete(f);
}
export async function pick(title: string, dir: boolean) {
  const v = window.prompt(`${title} (mock path)`, dir ? "D:\\ArmaMods" : "C:\\Games\\Arma 3\\arma3_x64.exe");
  return v || null;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const H = 3600_000;
const now = Date.now();
const EMPTY = new URLSearchParams(location.search).has("empty");

const state = (s: Partial<RepoConfig["state"]>): RepoConfig["state"] => ({
  revision: null, remote_revision: null, status: "unknown", last_check: null, last_sync: null, last_error: null, ...s,
});
const repo = (r: Partial<RepoConfig> & { name: string; url: string }): RepoConfig => ({
  game: "arma3", login: null, password: null, destination: "D:\\ArmaMods\\" + r.name.replace(/\s+/g, ""), exact_match: false,
  auto_discover: true, notify: true, auto: false, connections: 0, max_speed: 0, connect_timeout_secs: null, read_timeout_secs: null,
  accept_invalid_certs: false, keep_extras: [], prefer_modern: true, admin: null, state: state({}), ...r,
});

const repos: RepoConfig[] = EMPTY ? [] : [
  repo({ name: "Task Force Sunday", url: "https://mods.tfsunday.eu/r/main", state: state({ revision: 41, remote_revision: 43, status: "updated", last_check: now - 0.2 * H, last_sync: now - 50 * H }) }),
  repo({ name: "Vanilla+ Training", url: "https://mods.tfsunday.eu/r/training", notify: true, auto: true, state: state({ revision: 12, remote_revision: 12, status: "ok", last_check: now - 0.2 * H, last_sync: now - 200 * H }) }),
  repo({ name: "Old Ops Archive", url: "ftp://archive.example.org/arma3/", state: state({ revision: 7, remote_revision: 7, status: "error", last_check: now - 0.2 * H, last_sync: now - 2000 * H, last_error: "connection timed out (archive.example.org:21)" }) }),
];

const addonNames = ["@CBA_A3", "@ace", "@task_force_radio", "@rhsusaf", "@rhsafrf", "@rhsgref", "@cup_terrains_core", "@cup_terrains_maps", "@zen", "@lambs_danger", "@tfs_core", "@tfs_maps", "@enhanced_movement", "@3den_enhanced", "@acre2"];
const addons: Addon[] = EMPTY ? [] : [
  ...addonNames.map((n) => ({ key: n.toLowerCase(), name: n, display_name: n.replace("@", ""), path: `D:\\ArmaMods\\TaskForceSunday\\${n}`, source: { kind: "local" as const } })),
  { key: "@jsrs", name: "@jsrs", display_name: "JSRS SOUNDMOD", path: "C:\\Steam\\steamapps\\workshop\\content\\107410\\861133494", source: { kind: "workshop", id: 861133494 } },
  { key: "@blastcore", name: "@blastcore", display_name: "Blastcore Murr Edition", path: "C:\\Steam\\steamapps\\workshop\\content\\107410\\2257686620", source: { kind: "workshop", id: 2257686620 } },
];

const sel = (n: string, selected = true, optional = false): GroupNode => ({ type: "addon", name: n, selected, optional });
const defaultOptions = { game_profile: null, show_script_errors: false, no_pause: true, window_mode: false, file_patching: false, check_signatures: false, max_mem: null, cpu_count: null, ex_threads: null, malloc: null, enable_ht: false, huge_pages: false, no_splash: true, default_world_empty: true, no_logs: false, auto_restart: false };
const profiles: Record<string, Profile> = {
  Default: {
    name: "Default", executable: EMPTY ? null : "C:\\Steam\\steamapps\\common\\Arma 3\\arma3_x64.exe", launch_mode: "direct", proton: false,
    options: { ...defaultOptions }, additional_parameters: "", addon_search_dirs: EMPTY ? [] : ["D:\\ArmaMods"], use_workshop: true,
    addon_priority: ["@CBA_A3", "@ace"],
    groups: EMPTY ? [] : [
      { type: "group", name: "Task Force Sunday", modset: { kind: "repository", repository: "Task Force Sunday" }, children: ["@CBA_A3", "@ace", "@task_force_radio", "@rhsusaf", "@rhsafrf", "@cup_terrains_core", "@tfs_core", "@tfs_maps", "@zen"].map((n) => sel(n)) },
      { type: "group", name: "Sunday Op", modset: { kind: "event", repository: "Task Force Sunday", event: "Sunday Op" }, children: [sel("@lambs_danger", false, true), sel("@enhanced_movement", false, true)] },
      { type: "group", name: "Personal", children: [sel("@jsrs", true), sel("@blastcore", false), sel("@missing_mod", true)] },
    ],
    server: null,
  },
  Training: {
    name: "Training", executable: "C:\\Steam\\steamapps\\common\\Arma 3\\arma3_x64.exe", launch_mode: "steam", proton: false,
    options: { ...defaultOptions, window_mode: true }, additional_parameters: "-world=empty", addon_search_dirs: [], use_workshop: true, addon_priority: [],
    groups: [{ type: "group", name: "Vanilla+ Training", modset: { kind: "repository", repository: "Vanilla+ Training" }, children: [sel("@CBA_A3"), sel("@ace")] }],
    server: null,
  },
};

const config: AppStateOut["config"] = {
  default_destination: EMPTY ? null : "D:\\ArmaMods", check_interval_minutes: 30, proxy: null, max_speed: 0, segment_threshold: 64_000_000, profile: "Default",
  favorite_servers: EMPTY ? [] : [
    { name: "TFS Main", host: "play.tfsunday.eu", port: 2302, password: "sunday", modset: "Task Force Sunday", repository: "Task Force Sunday" },
    { name: "TFS Training", host: "play.tfsunday.eu", port: 2402, password: null, modset: "Vanilla+ Training", repository: null },
  ],
  external_apps: EMPTY ? [] : [{ name: "TeamSpeak 3", path: "C:\\Program Files\\TeamSpeak 3 Client\\ts3client_win64.exe", parameters: "", enabled: true }],
  at_launch: "minimize",
  updates: { check: true, auto_install: false, skipped: null },
  ui: { theme: new URLSearchParams(location.search).get("theme") ?? "dark", admin_tools: new URLSearchParams(location.search).has("admin"), sidebar_collapsed: false, onboarded: !EMPTY },
};

const syncing = new Map<string, ReturnType<typeof setInterval>>();

function appState(): AppStateOut {
  return {
    config: structuredClone(config), repos: structuredClone(repos), profiles: Object.keys(profiles), active_profile: config.profile ?? "Default",
    data_dir: "C:\\Users\\player\\AppData\\Roaming\\ArmaXSync", portable: false,
    game: { arma3_dir: "C:\\Steam\\steamapps\\common\\Arma 3", steam: true, workshop_dirs: ["C:\\Steam\\steamapps\\workshop\\content\\107410"], linux: false },
    syncing: [...syncing.keys()], version: "0.1.0", install_kind: "auto", release_repo: "ilbinek/ArmaXSync",
  };
}

function info(name: string): RepoInfo {
  if (name === "Old Ops Archive") throw "connection timed out (archive.example.org:21)";
  const r = repos.find((x) => x.name === name)!;
  return {
    revision: r.state.remote_revision ?? 1, source: "Modern", files: 18_432, size: 92_400_000_000,
    addons: addonNames.slice(0, 12),
    events: [
      { name: "Sunday Op", description: "Main operation, Sundays 19:00 CET", addons: [{ name: "@CBA_A3", optional: false }, { name: "@ace", optional: false }, { name: "@task_force_radio", optional: false }, { name: "@rhsusaf", optional: false }, { name: "@lambs_danger", optional: true }], userconfig: [] },
      { name: "Wednesday Training", description: "Lightweight, vanilla terrains", addons: [{ name: "@CBA_A3", optional: false }, { name: "@ace", optional: false }, { name: "@zen", optional: true }], userconfig: [] },
    ],
    changelogs: [
      { revision: 41, build_date: now - 60 * H, content_updated: true, addons: [], new_addons: [], updated_addons: ["@ace", "@CBA_A3"], deleted_addons: [] },
      { revision: 42, build_date: now - 30 * H, content_updated: true, addons: [], new_addons: ["@lambs_danger"], updated_addons: ["@tfs_core"], deleted_addons: ["@old_ai_mod"] },
      { revision: 43, build_date: now - 3 * H, content_updated: true, addons: [], new_addons: [], updated_addons: ["@tfs_maps", "@rhsusaf"], deleted_addons: [] },
    ],
    servers: [{ name: "TFS Main", host: "play.tfsunday.eu", port: 2302, password: "sunday", modset: "Task Force Sunday" }, { name: "TFS Zeus Sandbox", host: "play.tfsunday.eu", port: 2502 }],
  };
}

function startSync(name: string) {
  const total = 3_400_000_000;
  const files = ["@tfs_maps/addons/tfs_altis_ext.pbo", "@rhsusaf/addons/rhsusf_c_weapons.pbo", "@tfs_core/addons/tfs_core.pbo", "@ace/addons/ace_medical.pbo"];
  const p: SyncProgress = { repo: name, bytes_total: total, bytes_done: 0, files_total: 42, files_done: 0, failed: 0, current: files[0], speed: 0 };
  const t = setInterval(() => {
    const inc = 38_000_000 + Math.random() * 30_000_000;
    p.bytes_done = Math.min(total, p.bytes_done + inc);
    p.speed = inc * 4;
    p.files_done = Math.min(42, Math.floor((p.bytes_done / total) * 42));
    p.current = files[p.files_done % files.length];
    emit("sync-progress", { ...p });
    if (p.bytes_done >= total) {
      clearInterval(t);
      syncing.delete(name);
      const r = repos.find((x) => x.name === name)!;
      r.state = { ...r.state, revision: r.state.remote_revision, status: "ok", last_sync: Date.now() };
      emit("sync-done", { repo: name, ok: true, outcome: { report: { downloaded: 42, deleted: 3, failed: [], cancelled: false, bytes_transferred: total, bytes_reused: 0 } } });
    }
  }, 250);
  syncing.set(name, t);
}

export async function invoke(cmd: string, a: any = {}): Promise<any> {
  await sleep(cmd === "repo_info" || cmd === "sync_plan" || cmd === "repo_resolve" ? 500 : 60);
  switch (cmd) {
    case "app_state": return appState();
    case "save_config": Object.assign(config, a.config); return;
    case "repo_check_all":
      await sleep(700);
      repos.forEach((r) => (r.state.last_check = Date.now()));
      return repos.map((r) => ({ name: r.name, status: r.state.status, local_revision: r.state.revision, remote_revision: r.state.remote_revision, error: r.state.last_error }));
    case "repo_check": {
      await sleep(500);
      const r = repos.find((x) => x.name === a.name)!;
      r.state.last_check = Date.now();
      return { name: r.name, status: r.state.status, local_revision: r.state.revision, remote_revision: r.state.remote_revision, error: r.state.last_error };
    }
    case "repo_info": return info(a.name);
    case "repo_resolve":
      if (!/^(https?|ftp|armaxsync):/.test(a.url)) throw "not a repository URL";
      return repo({ name: "Community Mods", url: a.url.replace(/\/\.a3s\/autoconfig$/, ""), destination: null });
    case "repo_save": {
      const i = repos.findIndex((r) => r.name === (a.original ?? a.repo.name));
      if (a.original == null && i >= 0) throw `repository "${a.repo.name}" already exists`;
      if (i >= 0) repos[i] = a.repo; else repos.push(a.repo);
      return;
    }
    case "repo_remove": repos.splice(repos.findIndex((r) => r.name === a.name), 1); return;
    case "sync_plan": {
      const ops = [
        { op: "fetch", path: "@tfs_maps/addons/tfs_altis_ext.pbo", size: 1_840_000_000, sha1: "x", zsync: true, local_size: 1_790_000_000 },
        { op: "fetch", path: "@tfs_maps/addons/tfs_altis_ext.pbo.bisign", size: 570, sha1: "x", zsync: false, local_size: 570 },
        { op: "fetch", path: "@rhsusaf/addons/rhsusf_c_weapons.pbo", size: 412_000_000, sha1: "x", zsync: true, local_size: 409_000_000 },
        { op: "fetch", path: "@lambs_danger/addons/lambs_danger.pbo", size: 9_400_000, sha1: "x", zsync: false },
        { op: "fetch", path: "@lambs_danger/mod.cpp", size: 420, sha1: "x", zsync: false },
        { op: "delete", path: "@old_ai_mod", dir: true },
        { op: "delete", path: "@tfs_core/addons/tfs_legacy.pbo", dir: false },
      ];
      return { name: a.name, revision: 43, destination: "D:\\ArmaMods", plan: { revision: 43, ops, fetch_count: 5, fetch_bytes: 2_261_400_990, delete_count: 2 }, report: null };
    }
    case "sync_start": if (syncing.has(a.name)) throw `${a.name} is already synchronizing`; startSync(a.name); return;
    case "sync_cancel": {
      clearInterval(syncing.get(a.name));
      syncing.delete(a.name);
      emit("sync-done", { repo: a.name, ok: false, outcome: { report: { cancelled: true, failed: [], downloaded: 3, deleted: 0 } } });
      return;
    }
    case "profile_get": return structuredClone(profiles[a.name ?? config.profile ?? "Default"] ?? profiles.Default);
    case "profile_save":
      if (a.original && a.original !== a.profile.name) delete profiles[a.original];
      profiles[a.profile.name] = structuredClone(a.profile);
      return;
    case "profile_remove": delete profiles[a.name]; return;
    case "profile_activate": config.profile = a.name; return;
    case "addons": return addons;
    case "apply_modset": return profiles[a.profile];
    case "launch_preview": {
      const p: Profile = a.profile;
      const names = new Set(addons.map((x) => x.name.toLowerCase()));
      const selected: string[] = [];
      const walk = (n: GroupNode) => (n.type === "group" ? n.children.forEach(walk) : n.selected && selected.push(n.name));
      p.groups.forEach(walk);
      const missing = selected.filter((n) => !names.has(n.toLowerCase()));
      const srv = config.favorite_servers.find((s) => s.name === (a.server ?? p.server));
      const args = [
        "-noPause", "-noSplash", "-world=empty",
        `-mod=${selected.filter((n) => !missing.includes(n)).map((n) => `D:\\ArmaMods\\${n}`).join(";")}`,
        ...(srv ? [`-connect=${srv.host}`, `-port=${srv.port}`, ...(srv.password ? [`-password=${srv.password}`] : [])] : []),
      ];
      return {
        command: p.executable ? { program: p.executable, args, game_args: args } : null, args,
        error: p.executable ? null : "No game executable set",
        check: { missing_addons: missing, duplicate_selection: [] }, running: false,
      };
    }
    case "launch_game": return 4242;
    case "game_info": return { game_profiles: ["Player", "Zeus"], mallocs: ["tbb4malloc_bi_x64", "system"], running: false, cpus: 16 };
    case "extract_keys": return ["ace_3.17.1.bikey", "cba_3.16.bikey", "rhsusaf_0.5.6.bikey"];
    case "latest_rpt": return "C:\\Users\\player\\AppData\\Local\\Arma 3\\Arma3_x64_2026-11-19_20-14-03.rpt";
    case "read_text_from": {
      if (a.offset > 0) return { text: Math.random() < 0.3 ? `20:${(Math.random() * 60) | 0}:11 Warning Message: No entry 'bin\\config.bin/CfgVehicles/tfs_dummy'\n` : "", offset: a.offset + 1 };
      return { text: "=====================================================================\n== C:\\Steam\\steamapps\\common\\Arma 3\\arma3_x64.exe\n== \"arma3_x64.exe\" -noPause -mod=@CBA_A3;@ace\n\nOriginal output filename: Arma3Retail_DX11_x64\nExe timestamp: 2026/10/02 14:02:12\n20:14:05 Initializing stats manager.\n20:14:07 Warning Message: Addon 'tfs_maps' requires addon 'A3_Map_Altis_Ext'\n20:14:09 Error in expression <_unit setVariable [\"tfs_role\", _role];>\n20:14:09   Error position: <setVariable [\"tfs_role\", _role];>\n20:14:09   Error Undefined variable in expression: _role\n20:14:12 Mission id: 4f8a2c1e\n", offset: 1 };
    }
    case "server_helper": return { mods: ["D:\\Server\\mods\\@cba_a3", "D:\\Server\\mods\\@ace"], server_mods: [], keys: ["ace_3.17.1.bikey", "cba_3.16.bikey"], parameters: ["-mod=mods\\@cba_a3;mods\\@ace", "-serverMod="] };
    case "import_legacy": return { repositories: { repositories: ["Old Community"], skipped: [["Task Force Sunday", "already exists"]] }, profiles: ["Imported"], favorite_servers: 2, external_apps: 1, errors: [] };
    case "build_repo":
      for (let i = 0; i <= 10; i++) { emit("build-progress", [i * 1e9, 10e9]); await sleep(150); }
      return { revision: 44, files: 18_433, total_size: 92_400_000_000, hashed_files: 12, hashed_bytes: 1e9, zsync_written: 12, zips_written: 0, sidecars_removed: 0, content_updated: true, new_addons: [], updated_addons: ["@tfs_core"], deleted_addons: [] };
    case "push_repo":
      emit("push-progress", { phase: "hashing", done: 5, total: 10 }); await sleep(300);
      emit("push-progress", { phase: "planned", total: 420_000_000 }); await sleep(300);
      for (let i = 1; i <= 6; i++) { emit("push-progress", { phase: "uploading", path: `@tfs_core/addons/part${i}.pbo` }); emit("push-progress", { phase: "uploaded", bytes: 70_000_000 }); await sleep(250); }
      emit("push-progress", { phase: "publishing", job: 77 }); await sleep(400);
      return { changeset: 9, unchanged: 18_000, reused: 120, delta_files: 4, full_files: 2, deleted: 1, bytes_sent: 420_000_000, bytes_total: 2_100_000_000, revision: 44, job: 77 };
    case "update_check":
      await sleep(600);
      return { current: "0.1.0", version: "0.2.0", notes: "### Added\n- Redesigned interface with a dark tactical theme\n- Automatic updates\n\n### Fixed\n- One offline repository no longer blocks update checks", date: "2026-11-20", kind: "auto", url: "https://github.com/ilbinek/ArmaXSync/releases/tag/v0.2.0" };
    case "update_install":
      for (let i = 0; i <= 20; i++) { emit("update-progress", { downloaded: i * 2_000_000, total: 40_000_000, phase: "download" }); await sleep(100); }
      emit("update-progress", { downloaded: 0, total: null, phase: "install" });
      await sleep(800);
      location.reload();
      return;
  }
  throw `mock: unknown command ${cmd}`;
}
