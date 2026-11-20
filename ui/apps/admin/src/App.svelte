<script lang="ts">
  import { onMount } from "svelte";
  import { Toasts, Logo, Menu, Button, Spinner, Topo, CommandPalette, router, matchRoute, navigate, toast, toastError, setTheme, theme, type PaletteItem } from "@axs/kit";
  import { LayoutDashboard, Database, ListChecks, ChartColumn, Users as UsersIcon, ScrollText, KeyRound, Menu as MenuIcon, X, LogOut, ExternalLink, UserRound, Moon, Sun, Monitor, LoaderCircle, Search, Plus, Hammer, FolderSync, FolderTree, Settings2, ArrowUpCircle, UserPlus } from "@lucide/svelte";
  import { me as fetchMe, logout, onUnauthorized, onServerVersionChange, jobLabel, get, post, type RepoSummary, type Job } from "./api";
  import CreateRepo from "./components/CreateRepo.svelte";
  import { session, isAdmin, loadSystem, pollActive, openJob } from "./session.svelte";
  import Login from "./pages/Login.svelte";
  import Dashboard from "./pages/Dashboard.svelte";
  import Repos from "./pages/Repos.svelte";
  import Repo from "./pages/Repo.svelte";
  import Jobs from "./pages/Jobs.svelte";
  import Users from "./pages/Users.svelte";
  import Tokens from "./pages/Tokens.svelte";
  import Audit from "./pages/Audit.svelte";
  import Stats from "./pages/Stats.svelte";
  import JobDrawer from "./components/JobDrawer.svelte";
  import UpdateBanner from "./components/UpdateBanner.svelte";

  let loading = $state(true);
  let navOpen = $state(false);
  let expired = $state(false);
  let paletteOpen = $state(false);
  let createOpen = $state(false);
  let paletteRepos = $state<RepoSummary[]>([]);
  let paletteJobs = $state<Job[]>([]);

  async function openPalette() {
    paletteOpen = true;
    [paletteRepos, paletteJobs] = await Promise.all([get<RepoSummary[]>("/repos").catch(() => []), get<Job[]>("/jobs?limit=6").catch(() => [])]);
  }
  function globalKey(e: KeyboardEvent) {
    if (!session.me) return;
    const typing = (e.target as HTMLElement)?.closest?.("input, textarea, select, [contenteditable]");
    if (((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") || (e.key === "/" && !typing)) {
      e.preventDefault();
      paletteOpen ? (paletteOpen = false) : openPalette();
    }
  }
  async function run(path: string, label: string) {
    try {
      const r = await post<{ job: number }>(path);
      openJob(r.job);
    } catch (e) {
      toastError(e, label);
    }
  }
  const paletteItems = $derived.by((): PaletteItem[] => {
    const items: PaletteItem[] = nav.map((n) => ({ id: "p" + n.path, label: n.label, group: "Pages", icon: n.icon, run: () => navigate(n.path) }));
    for (const r of paletteRepos) {
      const base = `/repos/${encodeURIComponent(r.slug)}`;
      items.push({ id: "r" + r.slug, label: r.name, group: "Repositories", icon: Database, hint: `/r/${r.slug}`, keywords: r.slug, run: () => navigate(base) });
      items.push({ id: "rf" + r.slug, label: `${r.name}: files`, group: "Repositories", icon: FolderTree, keywords: `${r.slug} upload`, run: () => navigate(`${base}?tab=files`) });
      items.push({ id: "rs" + r.slug, label: `${r.name}: settings`, group: "Repositories", icon: Settings2, keywords: r.slug, run: () => navigate(`${base}?tab=settings`) });
      items.push({ id: "rt" + r.slug, label: `${r.name}: statistics`, group: "Repositories", icon: ChartColumn, keywords: `${r.slug} stats`, run: () => navigate(`${base}?tab=stats`) });
      items.push({ id: "rb" + r.slug, label: `Rebuild ${r.name}`, group: "Actions", weight: 5, icon: Hammer, keywords: `${r.slug} build publish`, run: () => run(`/repos/${encodeURIComponent(r.slug)}/build`, "Rebuild") });
      if (r.has_mirror) items.push({ id: "rm" + r.slug, label: `Pull ${r.name} from mirror`, group: "Actions", icon: FolderSync, keywords: r.slug, run: () => run(`/repos/${encodeURIComponent(r.slug)}/mirror`, "Mirror pull") });
    }
    if (admin) {
      items.push({ id: "a-new", label: "New repository", group: "Actions", icon: Plus, keywords: "create add", run: () => (createOpen = true) });
      items.push({ id: "a-user", label: "New user", group: "Actions", icon: UserPlus, keywords: "create add account", run: () => navigate("/users") });
      items.push({ id: "a-upd", label: "Check for server updates", group: "Actions", icon: ArrowUpCircle, keywords: "version release", run: () => post("/system/update/check").then(loadSystem).then(() => toast(session.system?.update?.available ? `ArmaXSync ${session.system.update.latest?.version} is available` : "The server is up to date")).catch((e) => toastError(e)) });
    }
    items.push({ id: "a-token", label: "New API token", group: "Actions", icon: KeyRound, keywords: "create push", run: () => navigate("/tokens") });
    items.push({ id: "t-dark", label: "Dark theme", group: "Preferences", icon: Moon, run: () => setTheme("dark") });
    items.push({ id: "t-light", label: "Light theme", group: "Preferences", icon: Sun, run: () => setTheme("light") });
    items.push({ id: "t-sys", label: "System theme", group: "Preferences", icon: Monitor, run: () => setTheme("system") });
    items.push({ id: "out", label: "Sign out", group: "Preferences", icon: LogOut, run: signOut });
    for (const j of paletteJobs) items.push({ id: "j" + j.id, label: `${jobLabel(j.kind)} #${j.id}${j.repo ? ` · ${j.repo}` : ""}`, group: "Recent jobs", icon: ListChecks, hint: j.status, weight: -15, run: () => openJob(j.id) });
    return items;
  });

  onMount(() => {
    (async () => {
      session.me = await fetchMe().catch(() => null);
      loading = false;
      if (session.me) await loadSystem();
    })();
    onUnauthorized(() => {
      if (!session.me) return;
      session.me = null;
      expired = true;
    });
    onServerVersionChange((v) => {
      if (session.restarting) return;
      toast(`The server was updated to ${v}`, "info", { ms: 0, action: { label: "Reload", run: () => location.reload() } });
    });
    const t = setInterval(() => session.me && pollActive(), 4000);
    const s = setInterval(() => session.me && loadSystem(), 5 * 60_000);
    return () => (clearInterval(t), clearInterval(s));
  });

  $effect(() => {
    if (session.me) pollActive();
  });
  // Close the mobile nav on navigation.
  $effect(() => {
    router.path;
    navOpen = false;
  });

  const admin = $derived(isAdmin());
  const nav = $derived([
    { path: "/", label: "Dashboard", icon: LayoutDashboard },
    { path: "/repos", label: "Repositories", icon: Database },
    { path: "/jobs", label: "Jobs", icon: ListChecks },
    { path: "/stats", label: "Statistics", icon: ChartColumn },
    ...(admin ? [{ path: "/users", label: "Users", icon: UsersIcon }, { path: "/audit", label: "Audit log", icon: ScrollText }] : []),
    { path: "/tokens", label: "API tokens", icon: KeyRound },
  ]);
  const repoParams = $derived(matchRoute("/repos/:slug"));
  const isActive = (p: string) => (p === "/" ? router.path === "/" : router.path.startsWith(p));

  async function signOut() {
    await logout();
    session.me = null;
    expired = false;
    navigate("/");
  }
  const ThemeIcon = $derived(theme.value === "light" ? Sun : theme.value === "system" ? Monitor : Moon);
</script>

<svelte:window onkeydown={globalKey} />
<Toasts />
{#if loading}
  <div class="center"><Logo size={44} /></div>
{:else if !session.me}
  <Login
    {expired}
    onlogin={async (m) => {
      session.me = m;
      expired = false;
      await loadSystem();
    }}
  />
{:else}
  <div class="layout">
    <header class="mbar">
      <button class="btn ghost icon" aria-label="Open navigation" onclick={() => (navOpen = true)}><MenuIcon size={20} /></button>
      <Logo size={26} wordmark />
    </header>
    {#if navOpen}<div class="scrim" role="presentation" onclick={() => (navOpen = false)}></div>{/if}
    <aside class:open={navOpen}>
      <div class="brandrow">
        <a class="brand" href="#/"><Logo size={32} wordmark sub="Server admin" /></a>
        <button class="btn ghost icon close" aria-label="Close navigation" onclick={() => (navOpen = false)}><X size={18} /></button>
      </div>
      <button class="search" onclick={openPalette} aria-label="Search (Ctrl+K)">
        <Search size={15} /><span class="grow">Search…</span><kbd>Ctrl</kbd><kbd>K</kbd>
      </button>
      <nav aria-label="Main">
        {#each nav as n}
          <a href={"#" + n.path} class:active={isActive(n.path)} aria-current={isActive(n.path) ? "page" : undefined}>
            <n.icon size={18} /><span>{n.label}</span>
            {#if n.path === "/jobs" && session.active.length}<span class="count">{session.active.length}</span>{/if}
          </a>
        {/each}
      </nav>
      <div class="spacer"></div>
      {#if session.active.length}
        <button class="running" onclick={() => openJob(session.active[0].id)}>
          <LoaderCircle size={15} class="spin" />
          <span class="grow ellipsis">{jobLabel(session.active[0].kind)}{session.active[0].repo ? ` · ${session.active[0].repo}` : ""}</span>
          {#if session.active.length > 1}<span class="xs subtle">+{session.active.length - 1}</span>{/if}
        </button>
      {/if}
      <div class="foot">
        <Menu
          align="left"
          items={[
            { label: "Dark theme", icon: Moon, onclick: () => setTheme("dark") },
            { label: "Light theme", icon: Sun, onclick: () => setTheme("light") },
            { label: "System theme", icon: Monitor, onclick: () => setTheme("system") },
            { divider: true },
            { label: "Public page", icon: ExternalLink, onclick: () => window.open("/", "_blank", "noopener") },
            { label: "Sign out", icon: LogOut, onclick: signOut },
          ]}
        >
          {#snippet trigger()}
            <span class="user">
              <span class="av"><UserRound size={16} /></span>
              <span class="grow" style="min-width:0">
                <span class="strong ellipsis" style="display:block">{session.me!.user.username}</span>
                <span class="xs subtle">{session.me!.user.role}{session.system ? ` · v${session.system.version}` : ""}</span>
              </span>
              <ThemeIcon size={15} class="subtle" />
            </span>
          {/snippet}
        </Menu>
      </div>
    </aside>

    <main>
      <Topo fixed tone="fg" opacity={0.035} fade="right" />
      <UpdateBanner />
      {#key router.path.split("/").slice(0, 3).join("/")}<div class="route">
      {#if repoParams}
        {#key repoParams.slug}<Repo slug={repoParams.slug} />{/key}
      {:else if router.path === "/repos"}
        <Repos />
      {:else if router.path === "/jobs"}
        <Jobs />
      {:else if router.path === "/users" && admin}
        <Users />
      {:else if router.path === "/tokens"}
        <Tokens />
      {:else if router.path === "/audit" && admin}
        <Audit />
      {:else if router.path === "/stats"}
        <Stats />
      {:else if router.path === "/"}
        <Dashboard />
      {:else}
        <div class="card"><div class="empty">Page not found. <a href="#/">Dashboard</a></div></div>
      {/if}
      </div>{/key}
    </main>
  </div>
  <JobDrawer />
  <CommandPalette bind:open={paletteOpen} items={paletteItems} />
  <CreateRepo bind:open={createOpen} />
{/if}

{#if session.restarting}
  <div class="restart">
    <div class="card stack" style="align-items:center;text-align:center;max-width:420px">
      <Spinner size={28} label="" />
      <h2 style="margin:0">Updating to {session.restarting}</h2>
      <p class="muted small">The server installs the update and restarts. This page reloads when it is back.</p>
      <Button size="sm" variant="ghost" onclick={() => (session.restarting = null)}>Hide</Button>
    </div>
  </div>
{/if}

<style>
  .center { display: grid; place-items: center; height: 100vh; }
  .layout { display: grid; grid-template-columns: var(--sidebar-w) minmax(0, 1fr); min-height: 100vh; }
  aside {
    position: sticky; top: 0; height: 100vh; display: flex; flex-direction: column; gap: 6px;
    background: var(--surface-1); border-right: 1px solid var(--line); padding: 14px 10px 10px; z-index: 40;
  }
  .brandrow { display: flex; align-items: center; padding: 2px 6px 14px; border-bottom: 1px solid var(--line); margin-bottom: 8px; }
  .brand { color: var(--fg); text-decoration: none; flex: 1; }
  .close { display: none; }
  nav { display: flex; flex-direction: column; gap: 2px; }
  nav a {
    display: flex; align-items: center; gap: 11px; height: 38px; padding: 0 11px; border-radius: var(--r); position: relative;
    color: var(--fg-2); text-decoration: none; font-family: var(--font-display); font-weight: 600; font-size: 15.5px; letter-spacing: 0.06em; text-transform: uppercase;
  }
  nav a:hover { background: var(--surface-2); color: var(--fg); }
  nav a.active { background: var(--accent-soft); color: var(--fg); }
  nav a.active::before { content: ""; position: absolute; left: -10px; top: 8px; bottom: 8px; width: 3px; border-radius: 0 3px 3px 0; background: var(--accent); }
  nav a.active :global(svg) { color: var(--accent); }
  .count { margin-left: auto; min-width: 20px; height: 20px; padding: 0 6px; border-radius: 99px; display: grid; place-items: center; background: var(--accent); color: var(--accent-fg); font: 700 11px/1 var(--font); }
  .spacer { flex: 1; }
  .running {
    display: flex; align-items: center; gap: 8px; padding: 9px 10px; border-radius: var(--r); border: 1px solid var(--accent-line);
    background: var(--accent-soft); color: var(--fg); font: inherit; font-size: 13px; cursor: pointer; text-align: left;
  }
  .running :global(.spin) { animation: axs-spin 0.9s linear infinite; color: var(--accent); }
  .foot { border-top: 1px solid var(--line); padding-top: 8px; }
  .foot :global(.menu) { display: flex; }
  .user { display: flex; align-items: center; gap: 10px; padding: 7px 8px; border-radius: var(--r); width: 100%; }
  .user:hover { background: var(--surface-2); }
  .av { width: 30px; height: 30px; border-radius: 50%; background: var(--surface-3); display: grid; place-items: center; color: var(--fg-2); flex-shrink: 0; }
  main { padding: 26px 30px 48px; min-width: 0; display: flex; flex-direction: column; gap: 18px; position: relative; }
  main > :global(:not(.topo)) { position: relative; z-index: 1; }
  .route { display: flex; flex-direction: column; gap: 18px; animation: route-in 220ms var(--ease); }
  @keyframes route-in { from { opacity: 0; transform: translateY(6px); } }
  .search {
    display: flex; align-items: center; gap: 8px; margin: 0 2px 6px; padding: 7px 10px; border-radius: var(--r);
    border: 1px solid var(--line); background: var(--bg); color: var(--fg-3); font: inherit; font-size: 13px; cursor: pointer;
  }
  .search:hover { border-color: var(--line-strong); color: var(--fg-2); }
  .search kbd { font-size: 10px; }
  .mbar { display: none; }
  .scrim { display: none; }
  .restart { position: fixed; inset: 0; z-index: 300; display: grid; place-items: center; background: var(--overlay); backdrop-filter: blur(3px); padding: 16px; }

  @media (max-width: 860px) {
    .layout { grid-template-columns: 1fr; }
    .mbar { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--line); background: var(--surface-1); position: sticky; top: 0; z-index: 30; }
    aside { position: fixed; left: 0; top: 0; bottom: 0; width: min(280px, 86vw); transform: translateX(-100%); transition: transform 180ms var(--ease); box-shadow: var(--shadow-lg); }
    aside.open { transform: none; }
    .close { display: inline-flex; }
    .scrim { display: block; position: fixed; inset: 0; background: var(--overlay); z-index: 35; }
    main { padding: 16px 14px 40px; }
  }
</style>
