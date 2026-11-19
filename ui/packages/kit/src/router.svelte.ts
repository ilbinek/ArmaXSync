// Minimal hash router: "#/repos/main?tab=files", with an optional navigation
// guard (unsaved changes).

function parse() {
  const h = typeof location === "undefined" ? "" : location.hash.replace(/^#/, "") || "/";
  const [path, q = ""] = h.split("?");
  return { path: path || "/", query: new URLSearchParams(q) };
}

export const router = $state(parse());

type Guard = () => boolean | Promise<boolean>;
const guards = new Set<Guard>();
let current = typeof location === "undefined" ? "" : location.hash;
let restoring = false;

async function allowed(): Promise<boolean> {
  for (const g of guards) if (!(await g())) return false;
  return true;
}

if (typeof window !== "undefined") {
  window.addEventListener("hashchange", async () => {
    if (restoring) {
      restoring = false;
      return;
    }
    const target = location.hash;
    if (guards.size) {
      // Put the old URL back while the guard decides.
      restoring = true;
      history.replaceState(null, "", current || "#/");
      restoring = false;
      if (!(await allowed())) return;
      history.replaceState(null, "", target);
    }
    current = target;
    const p = parse();
    router.path = p.path;
    router.query = p.query;
  });
  window.addEventListener("beforeunload", (e) => {
    if (dirtyCount > 0) {
      e.preventDefault();
      e.returnValue = "";
    }
  });
}

export function navigate(path: string) {
  location.hash = path;
}

/** Update a query parameter without a history entry or guard check. */
export function setQuery(key: string, value: string | null) {
  const q = new URLSearchParams(router.query);
  if (value == null || value === "") q.delete(key);
  else q.set(key, value);
  const s = q.toString();
  const h = `#${router.path}${s ? "?" + s : ""}`;
  history.replaceState(null, "", h);
  current = h;
  router.query = q;
}

/** Register a guard; returns a function that removes it. */
export function addNavigationGuard(g: Guard): () => void {
  guards.add(g);
  return () => guards.delete(g);
}

let dirtyCount = 0;
/** Count of components with unsaved changes (for beforeunload). */
export function markDirty(on: boolean) {
  dirtyCount = Math.max(0, dirtyCount + (on ? 1 : -1));
}

/** Match "/repos/:slug" against a path; returns params or null. */
export function matchRoute(pattern: string, path: string = router.path): Record<string, string> | null {
  const a = pattern.split("/").filter(Boolean);
  const b = path.split("/").filter(Boolean);
  if (a.length !== b.length && !pattern.endsWith("*")) return null;
  const params: Record<string, string> = {};
  for (let i = 0; i < a.length; i++) {
    if (a[i] === "*") {
      params["*"] = b.slice(i).map(decodeURIComponent).join("/");
      return params;
    }
    if (a[i].startsWith(":")) {
      if (b[i] === undefined) return null;
      params[a[i].slice(1)] = decodeURIComponent(b[i]);
    } else if (a[i] !== b[i]) return null;
  }
  return params;
}
