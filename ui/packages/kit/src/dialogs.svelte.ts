// Promise-based replacements for window.confirm / window.prompt, rendered by
// <DialogHost /> (mounted once per app).

export type ConfirmOptions = {
  title: string;
  message?: string;
  /** Bullet list under the message (e.g. what will be deleted). */
  details?: string[];
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  /** Optional checkbox; its state is returned by confirmWithOption(). */
  checkbox?: { label: string; checked?: boolean };
};

export type PromptOptions = {
  title: string;
  message?: string;
  label?: string;
  value?: string;
  placeholder?: string;
  confirmLabel?: string;
  /** Return an error message to block submission. */
  validate?: (v: string) => string | null | undefined;
};

type Pending =
  | { kind: "confirm"; opts: ConfirmOptions; resolve: (r: { ok: boolean; checked: boolean }) => void }
  | { kind: "prompt"; opts: PromptOptions; resolve: (r: string | null) => void };

export const dialogs = $state<{ current: Pending | null; queue: Pending[] }>({ current: null, queue: [] });

function push(p: Pending) {
  if (dialogs.current) dialogs.queue.push(p);
  else dialogs.current = p;
}

export function finishDialog() {
  dialogs.current = dialogs.queue.shift() ?? null;
}

export function confirmDialog(opts: ConfirmOptions | string): Promise<boolean> {
  const o = typeof opts === "string" ? { title: opts } : opts;
  return new Promise((resolve) => push({ kind: "confirm", opts: o, resolve: (r) => resolve(r.ok) }));
}

/** Confirm with a checkbox: resolves `null` when cancelled. */
export function confirmWithOption(opts: ConfirmOptions): Promise<{ checked: boolean } | null> {
  return new Promise((resolve) =>
    push({ kind: "confirm", opts, resolve: (r) => resolve(r.ok ? { checked: r.checked } : null) }),
  );
}

export function promptDialog(opts: PromptOptions | string): Promise<string | null> {
  const o = typeof opts === "string" ? { title: opts } : opts;
  return new Promise((resolve) => push({ kind: "prompt", opts: o, resolve }));
}
