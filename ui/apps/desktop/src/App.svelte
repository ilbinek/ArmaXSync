<script lang="ts">
  import { onMount } from "svelte";
  import { Toasts, Logo, Menu, Button, EmptyState, Topo, router, matchRoute, navigate, setTheme, bytes, type Theme } from "@axs/kit";
  import { Crosshair, Layers, Database, Server, Wrench, Settings as SettingsIcon, ChevronsLeft, ChevronsRight, Download, ArrowUpCircle, UserRound, Check, Plus, CircleAlert, RotateCw } from "@lucide/svelte";
  import { on } from "./api";
  import { app, refresh, loadProfile, checkAll, initEvents, switchProfile, saveConfig, checkForUpdate } from "./store.svelte";
  import Play from "./pages/Play.svelte";
  import Modsets from "./pages/Modsets.svelte";
  import Repositories from "./pages/Repositories.svelte";
  import RepoDetail from "./pages/RepoDetail.svelte";
  import Servers from "./pages/Servers.svelte";
  import Tools from "./pages/Tools.svelte";
  import Settings from "./pages/Settings.svelte";
  import AddRepo from "./components/AddRepo.svelte";
  import ActivityDrawer from "./components/ActivityDrawer.svelte";
  import UpdateDialog from "./components/UpdateDialog.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import NewProfile from "./components/NewProfile.svelte";

  let addUrl = $state<string | null>(null);
  let error = $state("");
  let activityOpen = $state(false);
  let updateOpen = $state(false);
  let newProfileOpen = $state(false);

  async function boot() {
    error = "";
    initEvents();
    try {
      await refresh();
      await loadProfile();
    } catch (e: any) {
      error = String(e);
      return;
    }
    const t = app.state!.config.ui.theme as Theme;
    if (t) setTheme(t);
    checkAll(true);
    if (app.state!.config.updates.check) setTimeout(() => checkForUpdate(true), 4000);
  }

  onMount(() => {
    boot();
    on<string>("deep-link", (url) => (addUrl = url));
    const upd = setInterval(() => app.state?.config.updates.check && checkForUpdate(true), 6 * 3600_000);
    return () => clearInterval(upd);
  });

  // Repository checks, re-armed whenever the interval setting changes.
  $effect(() => {
    const minutes = app.state?.config.check_interval_minutes ?? 0;
    if (minutes <= 0) return;
    const t = setInterval(() => checkAll(true), minutes * 60_000);
    return () => clearInterval(t);
  });

  const nav = [
    { path: "/", label: "Play", icon: Crosshair },
    { path: "/modsets", label: "Modsets", icon: Layers },
    { path: "/repos", label: "Repositories", icon: Database },
    { path: "/servers", label: "Servers", icon: Server },
    { path: "/tools", label: "Tools", icon: Wrench },
    { path: "/settings", label: "Settings", icon: SettingsIcon },
  ];
  const repoParams = $derived(matchRoute("/repos/:name"));
  const updates = $derived(app.state?.repos.filter((r) => r.state.status === "updated").length ?? 0);
  const errors = $derived(app.state?.repos.filter((r) => r.state.status === "error").length ?? 0);
  const active = $derived(Object.values(app.progress));
  const total = $derived(active.reduce((a, p) => ({ done: a.done + p.bytes_done, total: a.total + p.bytes_total, speed: a.speed + p.speed }), { done: 0, total: 0, speed: 0 }));
  let width = $state(1280);
  const collapsed = $derived((app.state?.config.ui.sidebar_collapsed ?? false) || width < 1000);
  const onboarding = $derived(!!app.state && !app.state.config.ui.onboarded && app.state.repos.length === 0);

  function isActive(path: string) {
    return path === "/" ? router.path === "/" : router.path.startsWith(path);
  }
</script>

<svelte:window bind:innerWidth={width} />
<Toasts />
{#if error}
  <div class="center">
    <div class="card" style="max-width:520px">
      <EmptyState icon={CircleAlert} title="ArmaXSync could not start" description={error}>
        <Button icon={RotateCw} onclick={boot}>Try again</Button>
      </EmptyState>
    </div>
  </div>
{:else if !app.state}
  <div class="center"><Logo size={44} /></div>
{:else if onboarding}
  <Onboarding onadd={() => (addUrl = "")} />
{:else}
  <div class="layout" class:collapsed>
    <aside>
      <a class="brand" href="#/" aria-label="ArmaXSync home">
        <Logo size={collapsed ? 30 : 32} wordmark={!collapsed} sub="Mod launcher" />
      </a>
      <nav aria-label="Main">
        {#each nav as n}
          <a href={"#" + n.path} class:active={isActive(n.path)} aria-current={isActive(n.path) ? "page" : undefined} data-tip={collapsed ? n.label : undefined} data-tip-pos="right">
            <n.icon size={18} />
            <span class="lbl">{n.label}</span>
            {#if n.path === "/repos" && (updates || errors)}
              <span class="count" class:err={!updates && errors}>{updates || errors}</span>
            {/if}
          </a>
        {/each}
      </nav>

      <div class="spacer"></div>

      {#if active.length}
        <button class="dl" onclick={() => (activityOpen = true)} data-tip={collapsed ? "Downloads" : undefined} data-tip-pos="right">
          <div class="row nowrap tight"><Download size={15} /><span class="lbl grow ellipsis">{active.length === 1 ? active[0].repo : `${active.length} downloads`}</span><span class="lbl xs tabular">{bytes(total.speed)}/s</span></div>
          <div class="mini"><div style:width="{total.total ? (total.done / total.total) * 100 : 0}%"></div></div>
        </button>
      {/if}

      {#if app.update}
        <button class="upd" onclick={() => (updateOpen = true)} data-tip={collapsed ? `Update to ${app.update.version}` : undefined} data-tip-pos="right">
          <ArrowUpCircle size={16} /><span class="lbl">Update to {app.update.version}</span>
        </button>
      {/if}

      <div class="foot">
        <Menu
          align="left"
          items={[
            ...app.state.profiles.map((p) => ({ label: p, icon: p === app.state!.active_profile ? Check : undefined, onclick: () => switchProfile(p) })),
            { divider: true as const },
            { label: "New profile…", icon: Plus, onclick: () => (newProfileOpen = true) },
            { label: "Profile settings", icon: SettingsIcon, onclick: () => navigate("/settings") },
          ]}
        >
          {#snippet trigger()}
            <span class="prof" data-tip={collapsed ? `Profile: ${app.state!.active_profile}` : undefined} data-tip-pos="right">
              <UserRound size={16} />
              <span class="lbl stack" style="gap:0">
                <span class="xs subtle">Profile</span>
                <span class="ellipsis strong">{app.state!.active_profile}</span>
              </span>
            </span>
          {/snippet}
        </Menu>
        <div class="row nowrap between ver">
          <button class="lbl linkish xs subtle" onclick={() => (activityOpen = true)}>v{app.state.version}{app.state.portable ? " · portable" : ""}</button>
          <button class="btn ghost icon sm" aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"} onclick={() => saveConfig({ ui: { ...app.state!.config.ui, sidebar_collapsed: !collapsed } })}>
            {#if collapsed}<ChevronsRight size={15} />{:else}<ChevronsLeft size={15} />{/if}
          </button>
        </div>
      </div>
    </aside>

    <main>
      <Topo fixed tone="fg" opacity={0.03} fade="right" />
      {#key router.path.split("/").slice(0, 3).join("/")}<div class="route">
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
        <Play onadd={() => (addUrl = "")} />
      {/if}
      </div>{/key}
    </main>
  </div>
{/if}

{#if addUrl !== null}
  <AddRepo
    url={addUrl}
    onclose={(name?: string) => {
      addUrl = null;
      if (name) navigate(`/repos/${encodeURIComponent(name)}`);
    }}
  />
{/if}
<ActivityDrawer bind:open={activityOpen} />
<UpdateDialog bind:open={updateOpen} />
<NewProfile bind:open={newProfileOpen} />

<style>
  .center { display: grid; place-items: center; height: 100vh; padding: 24px; }
  .layout { display: grid; grid-template-columns: var(--sidebar-w) 1fr; height: 100vh; transition: grid-template-columns 160ms var(--ease); }
  .layout.collapsed { grid-template-columns: 64px 1fr; }
  aside {
    background: var(--surface-1); border-right: 1px solid var(--line); display: flex; flex-direction: column;
    padding: 14px 10px 10px; gap: 6px; min-width: 0; overflow: visible; position: relative; z-index: 5;
  }
  .brand { padding: 4px 6px 14px; color: var(--fg); text-decoration: none; border-bottom: 1px solid var(--line); margin-bottom: 8px; }
  .collapsed .brand { padding: 4px 6px 14px; display: flex; justify-content: center; }
  nav { display: flex; flex-direction: column; gap: 2px; }
  nav a {
    display: flex; align-items: center; gap: 11px; height: 38px; padding: 0 11px; border-radius: var(--r);
    color: var(--fg-2); text-decoration: none; position: relative;
    font-family: var(--font-display); font-weight: 600; font-size: 15.5px; letter-spacing: 0.06em; text-transform: uppercase;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  nav a:hover { background: var(--surface-2); color: var(--fg); }
  nav a.active { background: var(--accent-soft); color: var(--fg); }
  nav a.active::before { content: ""; position: absolute; left: -10px; top: 8px; bottom: 8px; width: 3px; border-radius: 0 3px 3px 0; background: var(--accent); }
  nav a.active :global(svg) { color: var(--accent); }
  .count {
    margin-left: auto; min-width: 20px; height: 20px; padding: 0 6px; border-radius: 99px; display: grid; place-items: center;
    background: var(--accent); color: var(--accent-fg); font: 700 11px/1 var(--font); letter-spacing: 0;
  }
  .count.err { background: var(--err); color: #fff; }
  .collapsed .lbl, .collapsed .count { display: none; }
  .collapsed nav a { justify-content: center; padding: 0; }
  .collapsed .count { display: block; position: absolute; top: 4px; right: 6px; min-width: 8px; width: 8px; height: 8px; padding: 0; font-size: 0; }
  .spacer { flex: 1; }

  .dl, .upd {
    display: flex; flex-direction: column; gap: 6px; width: 100%; text-align: left; padding: 9px 10px;
    border-radius: var(--r); border: 1px solid var(--line); background: var(--surface-2); color: var(--fg); font: inherit; font-size: 13px; cursor: pointer;
  }
  .dl:hover, .upd:hover { border-color: var(--line-strong); }
  .dl :global(svg) { color: var(--accent); }
  .mini { height: 3px; background: var(--surface-3); border-radius: 2px; overflow: hidden; }
  .mini > div { height: 100%; background: var(--accent); transition: width 0.25s; }
  .upd { flex-direction: row; align-items: center; gap: 8px; border-color: var(--accent-line); background: var(--accent-soft); font-weight: 600; }
  .upd :global(svg) { color: var(--accent); }
  .collapsed .dl, .collapsed .upd { align-items: center; padding: 9px 0; }
  .collapsed .dl .row { justify-content: center; }

  .foot { border-top: 1px solid var(--line); padding-top: 8px; margin-top: 4px; display: flex; flex-direction: column; gap: 4px; }
  .foot :global(.menu) { display: flex; }
  .prof { display: flex; align-items: center; gap: 10px; padding: 7px 8px; border-radius: var(--r); width: 100%; min-width: 0; }
  .prof:hover { background: var(--surface-2); }
  .prof :global(svg) { color: var(--fg-2); }
  .collapsed .prof { justify-content: center; }
  .ver { padding: 0 2px 0 8px; }
  .collapsed .ver { justify-content: center; padding: 0; }
  .linkish { background: none; border: 0; padding: 0; cursor: pointer; font: inherit; }
  .linkish:hover { color: var(--fg-2); }

  main { overflow: auto; min-width: 0; padding: 26px 30px 40px; scrollbar-gutter: stable; position: relative; }
  .route { position: relative; z-index: 1; animation: route-in 220ms var(--ease); }
  @keyframes route-in { from { opacity: 0; transform: translateY(6px); } }
</style>
