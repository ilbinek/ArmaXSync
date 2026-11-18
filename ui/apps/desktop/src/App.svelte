<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Toasts, router, matchRoute, navigate } from "@axs/kit";
  import { on } from "./api";
  import { app, refresh, checkAll, initEvents } from "./store.svelte";
  import Play from "./pages/Play.svelte";
  import Modsets from "./pages/Modsets.svelte";
  import Repositories from "./pages/Repositories.svelte";
  import RepoDetail from "./pages/RepoDetail.svelte";
  import Servers from "./pages/Servers.svelte";
  import Tools from "./pages/Tools.svelte";
  import Settings from "./pages/Settings.svelte";
  import AddRepo from "./components/AddRepo.svelte";

  let addUrl = $state<string | null>(null);
  let timer: any;
  let error = $state("");

  onMount(async () => {
    initEvents();
    try {
      await refresh();
    } catch (e: any) {
      error = String(e);
      return;
    }
    on<string>("deep-link", (url) => {
      addUrl = url;
    });
    checkAll(true);
    const minutes = app.state?.config.check_interval_minutes || 0;
    if (minutes > 0) timer = setInterval(() => checkAll(), minutes * 60_000);
  });
  onDestroy(() => clearInterval(timer));

  const nav = [
    ["/", "Play", "▶"],
    ["/modsets", "Modsets", "▦"],
    ["/repos", "Repositories", "⇅"],
    ["/servers", "Servers", "◉"],
    ["/tools", "Tools", "⚒"],
    ["/settings", "Settings", "⚙"],
  ];
  const repoParams = $derived(matchRoute("/repos/:name"));
  const updates = $derived(app.state?.repos.filter((r) => r.state.status === "updated").length ?? 0);
  const syncing = $derived(Object.keys(app.progress).length);
</script>

<Toasts />
{#if error}
  <div class="empty"><div class="alert err">{error}</div></div>
{:else if !app.state}
  <div class="empty">Loading…</div>
{:else}
  <div class="layout">
    <aside>
      <div class="brand">ArmaXSync</div>
      <nav>
        {#each nav as [path, label, icon]}
          <a href={"#" + path} class:active={path === "/" ? router.path === "/" : router.path.startsWith(path)}>
            <span class="ic">{icon}</span>{label}
            {#if path === "/repos" && (updates || syncing)}<span class="badge {syncing ? 'info' : 'warn'}">{syncing ? "⟳" : updates}</span>{/if}
          </a>
        {/each}
      </nav>
      <div class="spacer"></div>
      <div class="foot small muted">
        v{app.state.version}{app.state.portable ? " · portable" : ""}<br />
        profile <b>{app.state.active_profile}</b>
      </div>
    </aside>
    <main>
      {#if repoParams}
        {#key repoParams.name}<RepoDetail name={repoParams.name} />{/key}
      {:else if router.path === "/modsets"}
        <Modsets />
      {:else if router.path === "/repos"}
        <Repositories onadd={() => (addUrl = "")} />
      {:else if router.path === "/servers"}
        <Servers />
      {:else if router.path === "/tools"}
        <Tools />
      {:else if router.path === "/settings"}
        <Settings />
      {:else}
        <Play />
      {/if}
    </main>
  </div>
{/if}

{#if addUrl !== null}
  <AddRepo url={addUrl} onclose={(name?: string) => { addUrl = null; if (name) navigate(`/repos/${encodeURIComponent(name)}`); }} />
{/if}

<style>
  .layout { display: grid; grid-template-columns: 190px 1fr; height: 100vh; }
  aside { background: var(--panel); border-right: 1px solid var(--line); display: flex; flex-direction: column; padding: 14px 10px; gap: 14px; }
  .brand { font-weight: 700; font-size: 17px; padding: 4px 8px; }
  nav { display: flex; flex-direction: column; gap: 2px; }
  nav a { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-radius: 7px; color: var(--fg); }
  nav a:hover { background: var(--panel-2); text-decoration: none; }
  nav a.active { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  nav .badge { margin-left: auto; }
  .ic { width: 18px; text-align: center; opacity: 0.8; }
  .spacer { flex: 1; }
  .foot { padding: 8px; border-top: 1px solid var(--line); }
  main { padding: 20px 22px; overflow: auto; min-width: 0; }
</style>
