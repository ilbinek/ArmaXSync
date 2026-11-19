// Theme: "dark" (default), "light" or "system". Stored per browser; the
// desktop app also mirrors it into its config.

export type Theme = "dark" | "light" | "system";
const KEY = "axs-theme";

function read(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "system" || v === "dark") return v;
  } catch {
    /* storage unavailable */
  }
  return "dark";
}

export const theme = $state<{ value: Theme }>({ value: typeof document === "undefined" ? "dark" : read() });

function apply(t: Theme) {
  if (typeof document !== "undefined") document.documentElement.dataset.theme = t;
}
apply(theme.value);

export function setTheme(t: Theme) {
  theme.value = t;
  apply(t);
  try {
    localStorage.setItem(KEY, t);
  } catch {
    /* ignore */
  }
}
