// Minimal hash router: "#/repos/main?tab=files".

function parse() {
  const h = typeof location === "undefined" ? "" : location.hash.replace(/^#/, "") || "/";
  const [path, q = ""] = h.split("?");
  return { path: path || "/", query: new URLSearchParams(q) };
}

export const router = $state(parse());

if (typeof window !== "undefined") {
  window.addEventListener("hashchange", () => {
    const p = parse();
    router.path = p.path;
    router.query = p.query;
  });
}

export function navigate(path: string) {
  location.hash = path;
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
