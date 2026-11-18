// Admin API client (session cookie + CSRF header).

export type Role = "owner" | "admin" | "user";
export type Me = { user: { user_id: number; username: string; role: Role }; csrf: string | null };

let csrf = "";

export class ApiError extends Error {
  constructor(public status: number, message: string) {
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
  if (r.status === 204) return undefined as T;
  const text = await r.text();
  let data: any = text;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    /* plain text */
  }
  if (!r.ok) throw new ApiError(r.status, (data && data.error) || r.statusText);
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
  await post("/auth/logout");
  csrf = "";
}

export function encPath(p: string) {
  return p.split("/").map(encodeURIComponent).join("/");
}

/** Upload one file into a changeset (direct PUT; tus for big files). */
export async function uploadFile(cs: number, path: string, file: File, onProgress?: (sent: number) => void): Promise<void> {
  if (file.size < 32 * 1024 * 1024) {
    await api("PUT", `/changesets/${cs}/files/${encPath(path)}`, undefined, file);
    onProgress?.(file.size);
    return;
  }
  const id = await tusUpload(file, onProgress);
  await post(`/changesets/${cs}/ops`, [{ op: "put", path, upload: id }]);
}

/** Resumable tus 1.0 upload; returns the upload id. */
export async function tusUpload(file: File, onProgress?: (sent: number) => void): Promise<string> {
  const headers = { "Tus-Resumable": "1.0.0", "x-csrf-token": csrf };
  const r = await fetch("/api/v1/uploads", { method: "POST", headers: { ...headers, "Upload-Length": String(file.size) }, credentials: "same-origin" });
  if (!r.ok) throw new ApiError(r.status, await r.text());
  const loc = r.headers.get("Location")!;
  const id = loc.split("/").pop()!;
  const chunk = 8 * 1024 * 1024;
  let offset = 0;
  let failures = 0;
  while (offset < file.size) {
    try {
      const body = file.slice(offset, Math.min(file.size, offset + chunk));
      const p = await fetch(loc, {
        method: "PATCH",
        headers: { ...headers, "Upload-Offset": String(offset), "Content-Type": "application/offset+octet-stream" },
        body,
        credentials: "same-origin",
      });
      if (!p.ok) throw new ApiError(p.status, await p.text());
      offset = Number(p.headers.get("Upload-Offset"));
      failures = 0;
      onProgress?.(offset);
    } catch (e) {
      if (++failures > 5) throw e;
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

/** Follow a job's live events (SSE). Returns a stop function. */
export function followJob(id: number, on: (e: JobEvent) => void): () => void {
  const es = new EventSource(`/api/v1/jobs/${id}/events`, { withCredentials: true });
  const handler = (m: MessageEvent) => {
    try {
      const e = JSON.parse(m.data) as JobEvent;
      on(e);
      if (e.type === "status" && e.status !== "running" && e.status !== "queued") es.close();
    } catch {
      /* ignore */
    }
  };
  for (const ev of ["log", "progress", "status"]) es.addEventListener(ev, handler as EventListener);
  es.onerror = () => es.close();
  return () => es.close();
}

/** Wait for a job to finish (polling). */
export async function waitJob(id: number): Promise<any> {
  for (;;) {
    const j = await get(`/jobs/${id}`);
    if (j.status !== "queued" && j.status !== "running") return j;
    await new Promise((r) => setTimeout(r, 700));
  }
}
