// Toast notifications plus a short activity history, so messages that
// disappeared can be read again.

export type ToastKind = "ok" | "err" | "info" | "warn";
export type ToastAction = { label: string; run: () => void };
export type Toast = { id: number; kind: ToastKind; text: string; detail?: string; action?: ToastAction; count: number };
export type ToastOptions = { ms?: number; detail?: string; action?: ToastAction };
export type Activity = { id: number; kind: ToastKind; text: string; detail?: string; at: number };

export const toasts = $state<Toast[]>([]);
export const activity = $state<Activity[]>([]);

const MAX_VISIBLE = 4;
const MAX_HISTORY = 60;
const timers = new Map<number, ReturnType<typeof setTimeout>>();
let next = 1;

export function dismissToast(id: number) {
  clearTimeout(timers.get(id));
  timers.delete(id);
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}

function schedule(id: number, ms: number) {
  clearTimeout(timers.get(id));
  if (ms > 0) timers.set(id, setTimeout(() => dismissToast(id), ms));
}

/**
 * Show a toast. `opts` may be a duration in ms (legacy) or options.
 * Errors stay longer by default; identical visible toasts are merged.
 */
export function toast(text: string, kind: ToastKind = "ok", opts: number | ToastOptions = {}) {
  const o: ToastOptions = typeof opts === "number" ? { ms: opts } : opts;
  const ms = o.ms ?? (kind === "err" ? 9000 : o.action ? 7000 : 4500);
  activity.unshift({ id: next, kind, text, detail: o.detail, at: Date.now() });
  if (activity.length > MAX_HISTORY) activity.length = MAX_HISTORY;

  const same = toasts.find((t) => t.text === text && t.kind === kind);
  if (same) {
    same.count++;
    schedule(same.id, ms);
    return same.id;
  }
  const id = next++;
  toasts.push({ id, kind, text, detail: o.detail, action: o.action, count: 1 });
  while (toasts.length > MAX_VISIBLE) dismissToast(toasts[0].id);
  schedule(id, ms);
  return id;
}

/** Error helper: accepts anything thrown. */
export function toastError(e: unknown, prefix?: string) {
  const msg = errorText(e);
  return toast(prefix ? `${prefix}: ${msg}` : msg, "err");
}

export function errorText(e: unknown): string {
  if (e == null) return "Unknown error";
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && "message" in e) return String((e as any).message);
  return String(e);
}
