import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

export type RepoStatus = "ok" | "updated" | "error" | "unknown";

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
  admin: any | null;
  state: { revision: number | null; remote_revision: number | null; status: RepoStatus; last_check: number | null; last_sync: number | null; last_error: string | null };
};

export type GroupNode =
  | { type: "group"; name: string; children: GroupNode[]; modset?: { kind: string; repository: string; event?: string } | null }
  | { type: "addon"; name: string; selected: boolean; optional: boolean };

export type Profile = {
  name: string;
  executable: string | null;
  launch_mode: "direct" | "steam";
  proton: boolean;
  options: Record<string, any>;
  additional_parameters: string;
  addon_search_dirs: string[];
  use_workshop: boolean;
  addon_priority: string[];
  groups: GroupNode[];
  server: string | null;
};

export type Addon = { key: string; name: string; display_name: string; path: string; source: { kind: string; id?: number } };

export type AppStateOut = {
  config: any;
  repos: RepoConfig[];
  profiles: string[];
  active_profile: string;
  data_dir: string;
  portable: boolean;
  game: { arma3_dir: string | null; steam: boolean; workshop_dirs: string[]; linux: boolean };
  syncing: string[];
  version: string;
};

export const call = <T = any>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args);
export const on = <T = any>(ev: string, f: (p: T) => void): Promise<UnlistenFn> => listen<T>(ev, (e) => f(e.payload));

export async function pickFolder(title = "Select folder"): Promise<string | null> {
  const r = await openDialog({ directory: true, multiple: false, title });
  return typeof r === "string" ? r : null;
}

export async function pickFile(title = "Select file", extensions?: string[]): Promise<string | null> {
  const r = await openDialog({ directory: false, multiple: false, title, filters: extensions ? [{ name: "Files", extensions }] : undefined });
  return typeof r === "string" ? r : null;
}

export function selectedAddons(groups: GroupNode[]): string[] {
  const out: string[] = [];
  const walk = (n: GroupNode) => {
    if (n.type === "group") n.children.forEach(walk);
    else if (n.selected) out.push(n.name);
  };
  groups.forEach(walk);
  return out;
}
