// Focus management for dialogs: trap Tab inside, focus the first field (or
// [autofocus]) on open, restore focus to the opener on close.

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function trapFocus(node: HTMLElement) {
  const opener = document.activeElement as HTMLElement | null;
  const items = () => Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((e) => e.offsetParent !== null);

  queueMicrotask(() => {
    const auto = node.querySelector<HTMLElement>("[data-autofocus]");
    const field = node.querySelector<HTMLElement>(".body input:not([type=checkbox]):not([disabled]), .body select, .body textarea");
    (auto ?? field ?? items()[0] ?? node).focus();
  });

  function key(e: KeyboardEvent) {
    if (e.key !== "Tab") return;
    const f = items();
    if (!f.length) return;
    const first = f[0];
    const last = f[f.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
  node.addEventListener("keydown", key);
  return {
    destroy() {
      node.removeEventListener("keydown", key);
      if (opener && document.contains(opener)) opener.focus();
    },
  };
}
