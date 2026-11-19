import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl as tauriOpenUrl, openPath as tauriOpenPath } from "@tauri-apps/plugin-opener";

/** `VITE_MOCK=1 pnpm dev:desktop` runs the UI in a browser with fake data. */
export const MOCK = import.meta.env.VITE_MOCK === "1";
const mock = MOCK ? import("./mock") : null;

export type RepoStatus = "ok" | "updated" | "error" | "unknown";

export type AdminSettings = {
  path: string | null;
  public_url: string | null;
  compress: boolean;
  compressed_only: boolean;
  connections: number;
  partial_transfer: boolean;
  embed_credentials: boolean;
  exclude: string[];
  hidden: string[];
  servers: any[];
  push_server: string | null;
  push_repo: string | null;
  push_token: string | null;
};

export type RepoState = {
  revision: number | null;
  remote_revision: number | null;
  status: RepoStatus;
  last_check: number | null;
  last_sync: number | null;
  last_error: string | null;
};

export type RepoConfig = {
  name: string;
  game: string;
  url: string;
  login: string | null;
  password: string | null;
  destination: string | null;
  exact_match: boolean;
  auto_discover: boolean;
  notify: boolean;
  auto: boolean;
  connections: number;
  max_speed: number;
  connect_timeout_secs: number | null;
  read_timeout_secs: number | null;
  accept_invalid_certs: boolean;
  keep_extras: string[];
  prefer_modern: boolean;
  admin: AdminSettings | null;
  state: RepoState;
};

export type ModsetRef = { kind: "repository"; repository: string } | { kind: "event"; repository: string; event: string };

export type GroupNode =
  | { type: "group"; name: string; children: GroupNode[]; modset?: ModsetRef | null }
  | { type: "addon"; name: string; selected: boolean; optional: boolean };

export type LauncherOptions = {
  game_profile: string | null;
  show_script_errors: boolean;
  no_pause: boolean;
  window_mode: boolean;
  file_patching: boolean;
  check_signatures: boolean;
  max_mem: number | null;
  cpu_count: number | null;
  ex_threads: number | null;
  malloc: string | null;
  enable_ht: boolean;
  huge_pages: boolean;
  no_splash: boolean;
  default_world_empty: boolean;
  no_logs: boolean;
  auto_restart: boolean;
};

export type Profile = {
  name: string;
  executable: string | null;
  launch_mode: "direct" | "steam";
  proton: boolean;
  options: LauncherOptions;
  additional_parameters: string;
  addon_search_dirs: string[];
  use_workshop: boolean;
  addon_priority: string[];
  groups: GroupNode[];
  server: string | null;
};

export type Addon = { key: string; name: string; display_name: string; path: string; source: { kind: "local" } | { kind: "workshop"; id: number } };

export type FavoriteServer = { name: string; host: string; port: number; password: string | null; modset: string | null; repository: string | null };
export type ExternalApp = { name: string; path: string; parameters: string; enabled: boolean };

export type AppConfig = {
  default_destination: string | null;
  check_interval_minutes: number;
  proxy: string | null;
  max_speed: number;
  segment_threshold: number;
  profile: string | null;
  favorite_servers: FavoriteServer[];
  external_apps: ExternalApp[];
  at_launch: "nothing" | "minimize" | "close";
  updates: { check: boolean; auto_install: boolean; skipped: string | null };
  ui: { theme: string; admin_tools: boolean; sidebar_collapsed: boolean; onboarded: boolean };
};

export type InstallKind = "auto" | "manual" | "dev";

export type AppStateOut = {
  config: AppConfig;
  repos: RepoConfig[];
  profiles: string[];
  active_profile: string;
  data_dir: string;
  portable: boolean;
  game: { arma3_dir: string | null; steam: boolean; workshop_dirs: string[]; linux: boolean };
  syncing: string[];
  version: string;
  install_kind: InstallKind;
  release_repo: string;
};

export type EventDef = { name: string; description?: string | null; addons: { name: string; optional: boolean }[]; userconfig: { name: string; optional: boolean }[] };
export type ChangelogEntry = { revision: number; build_date: number; content_updated: boolean; addons: string[]; new_addons: string[]; updated_addons: string[]; deleted_addons: string[] };
export type ServerDef = { name: string; host: string; port: number; password?: string | null; modset?: string | null };

export type RepoInfo = {
  revision: number;
  source: string;
  events: EventDef[];
  changelogs: ChangelogEntry[];
  addons: string[];
  servers: ServerDef[];
  files: number;
  size: number;
};

export type PlanOp =
  | { op: "mkdir"; path: string }
  | { op: "fetch"; path: string; size: number; sha1: string; zsync: boolean; local_size?: number }
  | { op: "delete"; path: string; dir: boolean };

export type SyncOutcome = {
  name: string;
  revision: number;
  destination: string;
  plan: { revision: number; ops: PlanOp[]; fetch_count: number; fetch_bytes: number; delete_count: number };
  report: { downloaded: number; failed: [string, string][]; deleted: number; bytes_transferred: number; bytes_reused: number; cancelled: boolean } | null;
};

export type SyncProgress = {
  repo: string;
  bytes_total: number;
  bytes_done: number;
  files_total: number;
  files_done: number;
  failed: number;
  current: string;
  speed: number;
};

export type CheckOutcome = { name: string; status: RepoStatus; local_revision: number | null; remote_revision: number | null; error: string | null };

export type Preview = {
  command: { program: string; args: string[]; game_args: string[] } | null;
  args: string[];
  error: string | null;
  check: { missing_addons: string[]; duplicate_selection: string[] };
  running: boolean;
};

export type GameDetails = { game_profiles: string[]; mallocs: string[]; running: boolean; cpus: number };

export type UpdateInfo = { current: string; version: string; notes: string; date: string | null; kind: InstallKind; url: string };

export async function call<T = any>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (mock) return (await mock).invoke(cmd, args) as Promise<T>;
  return invoke<T>(cmd, args);
}

export async function on<T = any>(ev: string, f: (p: T) => void): Promise<UnlistenFn> {
  if (mock) return (await mock).listen(ev, f as any);
  return listen<T>(ev, (e) => f(e.payload));
}

export async function pickFolder(title = "Select folder"): Promise<string | null> {
  if (mock) return (await mock).pick(title, true);
  const r = await openDialog({ directory: true, multiple: false, title });
  return typeof r === "string" ? r : null;
}

export async function pickFile(title = "Select file", extensions?: string[]): Promise<string | null> {
  if (mock) return (await mock).pick(title, false);
  const r = await openDialog({ directory: false, multiple: false, title, filters: extensions ? [{ name: "Files", extensions }] : undefined });
  return typeof r === "string" ? r : null;
}

export async function openUrl(url: string) {
  if (mock) return void window.open(url, "_blank", "noopener");
  await tauriOpenUrl(url);
}

/** Open a folder or file with the system's default handler. */
export async function openPath(path: string) {
  if (mock) return void console.info("mock openPath", path);
  await tauriOpenPath(path);
}

export function selectedAddons(groups: GroupNode[]): string[] {
  const out: string[] = [];
  const walk = (n: GroupNode) => {
    if (n.type === "group") n.children.forEach(walk);
    else if (n.selected) out.push(n.name);
  };
  groups.forEach(walk);
  return [...new Set(out)];
}

export function allAddonNames(groups: GroupNode[]): string[] {
  const out: string[] = [];
  const walk = (n: GroupNode) => (n.type === "group" ? n.children.forEach(walk) : out.push(n.name));
  groups.forEach(walk);
  return out;
}
