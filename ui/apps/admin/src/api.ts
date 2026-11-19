// Admin API client (session cookie + CSRF header).

export type Role = "owner" | "admin" | "user";
export type Me = { user: { user_id: number; username: string; role: Role }; csrf: string | null };

export type RepoSummary = {
  slug: string;
  name: string;
  description: string;
  game: string;
  mode: "managed" | "external";
  visibility: "public" | "private";
  revision: number;
  build_date: number;
  files: number;
  size: number;
  url: string;
  autoconfig_url: string;
  deep_link: string;
  bytes_served: number;
  requests: number;
  has_mirror: boolean;
};

export type ServerDef = { name: string; host: string; port: number; password?: string | null; modset?: string | null };
export type EventDef = { name: string; description?: string | null; addons: { name: string; optional: boolean }[]; userconfig: { name: string; optional: boolean }[] };
export type MirrorSettings = { url: string; login: string | null; password_sealed: string | null; import_events: boolean; import_hidden: boolean; import_servers: boolean; include: string[] };
export type RepoSettings = {
  visibility: "public" | "private";
  compress: boolean;
  compressed_only: boolean;
  connections: number;
  partial_transfer: boolean;
  embed_credentials: boolean;
  hidden: string[];
  exclude: string[];
  servers: ServerDef[];
  events: EventDef[];
  mirror: MirrorSettings | null;
  bandwidth_limit: number;
  max_downloads: number;
  listed: boolean;
};
export type RepoDetail = RepoSummary & { settings: RepoSettings; path: string | null; created_at: number };

export type Job = {
  id: number;
  repo: string | null;
  kind: string;
  status: "queued" | "running" | "succeeded" | "failed" | "cancelled" | "interrupted";
  created_at: number;
  started_at: number | null;
  finished_at: number | null;
  summary: any;
  error: string | null;
  log?: string;
};

export type UpdateStatus = {
  current: string;
  target: string;
  install_kind: "systemd" | "docker" | "manual";
  enabled: boolean;
  auto_install: boolean;
  checked_at: number | null;
  error: string | null;
  latest: { version: string; notes: string; pub_date: string | null; url: string; has_build: boolean } | null;
  available: boolean;
  can_install: boolean;
  last_result: { status: string; version: string; message: string; at: number } | null;
  staged: string | null;
};
export type SystemInfo = { version: string; public_url: string; started: number; repositories: number; update?: UpdateStatus };

let csrf = "";
/** Server version seen on the first response; a change means the server was updated. */
let firstVersion: string | null = null;
const listeners = { unauthorized: [] as (() => void)[], version: [] as ((v: string) => void)[] };

export function onUnauthorized(f: () => void) {
  listeners.unauthorized.push(f);
}
export function onServerVersionChange(f: (v: string) => void) {
  listeners.version.push(f);
}

function noteVersion(r: Response) {
  const v = r.headers.get("x-axs-version");
  if (!v) return;
  if (firstVersion === null) firstVersion = v;
  else if (v !== firstVersion) {
    listeners.version.forEach((f) => f(v));
    firstVersion = v;
  }
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

export async function api<T = any>(method: string, path: string, body?: unknown, raw?: BodyInit): Promise<T> {
  const headers: Record<string, string> = {};
  if (csrf) headers["x-csrf-token"] = csrf;
  let payload: BodyInit | undefined = raw;
  if (body !== undefined) {
    headers["content-type"] = "application/json";
    payload = JSON.stringify(body);
  }
  const r = await fetch(`/api/v1${path}`, { method, headers, body: payload, credentials: "same-origin" });
  noteVersion(r);
  if (r.status === 204) return undefined as T;
  const text = await r.text();
  let data: any = text;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    /* plain text */
  }
  if (!r.ok) {
    if (r.status === 401 && path !== "/me" && path !== "/auth/login" && csrf) listeners.unauthorized.forEach((f) => f());
    throw new ApiError(r.status, (data && data.error) || r.statusText);
  }
  return data as T;
}

export const get = <T = any>(p: string) => api<T>("GET", p);
export const post = <T = any>(p: string, b?: unknown) => api<T>("POST", p, b ?? {});
export const put = <T = any>(p: string, b?: unknown) => api<T>("PUT", p, b);
export const patch = <T = any>(p: string, b?: unknown) => api<T>("PATCH", p, b);
export const del = <T = any>(p: string) => api<T>("DELETE", p);

export async function me(): Promise<Me | null> {
  try {
    const m = await get<Me>("/me");
    csrf = m.csrf ?? "";
    return m;
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) return null;
    throw e;
  }
}

export async function login(username: string, password: string): Promise<Me> {
  const m = await post<Me>("/auth/login", { username, password });
  csrf = m.csrf ?? "";
  return m;
}

export async function logout() {
  await post("/auth/logout").catch(() => {});
  csrf = "";
}

export function encPath(p: string) {
  return p.split("/").map(encodeURIComponent).join("/");
}

/** Upload one file into a changeset (direct PUT; tus for big files). */
export async function uploadFile(cs: number, path: string, file: File, onProgress?: (sent: number) => void, signal?: AbortSignal): Promise<void> {
  if (file.size < 32 * 1024 * 1024) {
    const headers: Record<string, string> = csrf ? { "x-csrf-token": csrf } : {};
    const r = await fetch(`/api/v1/changesets/${cs}/files/${encPath(path)}`, { method: "PUT", headers, body: file, credentials: "same-origin", signal });
    if (!r.ok) throw new ApiError(r.status, (await r.json().catch(() => null))?.error ?? r.statusText);
    onProgress?.(file.size);
    return;
  }
  const id = await tusUpload(file, onProgress, signal);
  await post(`/changesets/${cs}/ops`, [{ op: "put", path, upload: id }]);
}

/** Resumable tus 1.0 upload; returns the upload id. */
export async function tusUpload(file: File, onProgress?: (sent: number) => void, signal?: AbortSignal): Promise<string> {
  const headers = { "Tus-Resumable": "1.0.0", "x-csrf-token": csrf };
  const r = await fetch("/api/v1/uploads", { method: "POST", headers: { ...headers, "Upload-Length": String(file.size) }, credentials: "same-origin", signal });
  if (!r.ok) throw new ApiError(r.status, await r.text());
  const loc = r.headers.get("Location")!;
  const id = loc.split("/").pop()!;
  const chunk = 8 * 1024 * 1024;
  let offset = 0;
  let failures = 0;
  while (offset < file.size) {
    if (signal?.aborted) throw new DOMException("cancelled", "AbortError");
    try {
      const body = file.slice(offset, Math.min(file.size, offset + chunk));
      const p = await fetch(loc, {
        method: "PATCH",
        headers: { ...headers, "Upload-Offset": String(offset), "Content-Type": "application/offset+octet-stream" },
        body,
        credentials: "same-origin",
        signal,
      });
      if (!p.ok) throw new ApiError(p.status, await p.text());
      offset = Number(p.headers.get("Upload-Offset"));
      failures = 0;
      onProgress?.(offset);
    } catch (e) {
      if ((e as any)?.name === "AbortError" || ++failures > 5) throw e;
      await new Promise((res) => setTimeout(res, 1000 * failures));
      const h = await fetch(loc, { method: "HEAD", headers, credentials: "same-origin" });
      offset = Number(h.headers.get("Upload-Offset") ?? offset);
    }
  }
  return id;
}

export type JobEvent =
  | { type: "log"; line: string }
  | { type: "progress"; phase: string; done: number; total: number }
  | { type: "status"; status: string };

const finished = (s: string) => s !== "running" && s !== "queued";

/**
 * Follow a job's live events (SSE). Reconnects after network errors and
 * falls back to polling, so a dropped connection never freezes the view.
 * Returns a stop function.
 */
export function followJob(id: number, on: (e: JobEvent) => void): () => void {
  let stopped = false;
  let es: EventSource | null = null;
  let tries = 0;
  let timer: ReturnType<typeof setTimeout>;

  const open = () => {
    if (stopped) return;
    es = new EventSource(`/api/v1/jobs/${id}/events`, { withCredentials: true });
    const handler = (m: MessageEvent) => {
      try {
        const e = JSON.parse(m.data) as JobEvent;
        tries = 0;
        on(e);
        if (e.type === "status" && finished(e.status)) stop();
      } catch {
        /* ignore */
      }
    };
    for (const ev of ["log", "progress", "status"]) es.addEventListener(ev, handler as EventListener);
    es.onerror = async () => {
      es?.close();
      if (stopped) return;
      // The job may have finished while we were disconnected.
      try {
        const j = await get<Job>(`/jobs/${id}`);
        if (finished(j.status)) {
          on({ type: "status", status: j.status });
          return stop();
        }
      } catch {
        /* offline: retry below */
      }
      timer = setTimeout(open, Math.min(10_000, 1000 * 2 ** tries++));
    };
  };
  const stop = () => {
    stopped = true;
    clearTimeout(timer);
    es?.close();
  };
  open();
  return stop;
}

/** Wait for a job to finish (polling). */
export async function waitJob(id: number): Promise<Job> {
  for (;;) {
    const j = await get<Job>(`/jobs/${id}`);
    if (finished(j.status)) return j;
    await new Promise((r) => setTimeout(r, 700));
  }
}

export const jobKinds: Record<string, string> = {
  build: "Rebuild",
  publish: "Publish",
  mirror: "Mirror pull",
  gc: "Housekeeping",
  self_update: "Server update",
};
export const jobLabel = (k: string) => jobKinds[k] ?? k;
