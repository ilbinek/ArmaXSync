<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Tabs, Button, Badge, EmptyState, Alert, Menu, ago, confirmDialog, navigate, toastError, errorText } from "@axs/kit";
  import { Database, Download, RefreshCw, X, LayoutDashboard, CalendarDays, History, Settings2, Server as ServerIcon, FolderOpen, Trash2, Link } from "@lucide/svelte";
  import { call, openPath, type RepoInfo } from "../api";
  import { app, refresh, startSync, cancelSync, checkRepo } from "../store.svelte";
  import Overview from "./repo/Overview.svelte";
  import SyncTab from "./repo/SyncTab.svelte";
  import Events from "./repo/Events.svelte";
  import Changelog from "./repo/Changelog.svelte";
  import RepoSettings from "./repo/RepoSettings.svelte";
  import Hosting from "./repo/Hosting.svelte";

  let { name }: { name: string } = $props();
  let tab = $state<string>();
  let info = $state<RepoInfo | null>(null);
  let infoError = $state<string | null>(null);
  let loadingInfo = $state(true);

  const repo = $derived(app.state?.repos.find((r) => r.name === name));
  const progress = $derived(app.progress[name]);

  async function loadInfo() {
    loadingInfo = true;
    infoError = null;
    try {
      info = await call<RepoInfo>("repo_info", { name });
    } catch (e) {
      infoError = errorText(e);
    } finally {
      loadingInfo = false;
    }
  }
  onMount(loadInfo);

  async function sync(event: string | null = null) {
    try {
      await startSync(name, { event });
    } catch (e) {
      toastError(e);
    }
  }

  async function remove() {
    if (!(await confirmDialog({ title: `Remove ${name}?`, message: "The repository is removed from ArmaXSync. Downloaded files stay on disk.", confirmLabel: "Remove", danger: true }))) return;
    try {
      await call("repo_remove", { name });
      await refresh();
      navigate("/repos");
    } catch (e) {
      toastError(e);
    }
  }

  const st = $derived(
    !repo
      ? null
      : progress
        ? { tone: "accent" as const, label: "Syncing" }
        : repo.state.status === "updated"
          ? { tone: "warn" as const, label: `Revision ${repo.state.remote_revision} available` }
          : repo.state.status === "ok"
            ? { tone: "ok" as const, label: "Up to date" }
            : repo.state.status === "error"
              ? { tone: "err" as const, label: "Unreachable" }
              : { tone: "neutral" as const, label: "Not checked" },
  );
  const hosting = $derived(!!repo && (!!repo.admin || !!app.state?.config.ui.admin_tools));
</script>

{#if !repo}
  <div class="card">
    <EmptyState icon={Database} title="Repository not found" description={`“${name}” is not in your list.`}>
      <Button href="#/repos">Back to repositories</Button>
    </EmptyState>
  </div>
{:else}
  <div class="page">
    <PageHeader title={repo.name} crumbs={[{ label: "Repositories", href: "#/repos" }]} icon={Database}>
      {#snippet meta()}
        {#if st}<Badge tone={st.tone} dot pulse={!!progress}>{st.label}</Badge>{/if}
        <span class="row tight nowrap"><Link size={13} /><span class="mono ellipsis" style="max-width:420px" title={repo.url}>{repo.url}</span></span>
        <span>Checked {ago(repo.state.last_check)}</span>
      {/snippet}
      {#snippet actions()}
        {#if progress}
          <Button icon={X} onclick={() => cancelSync(name)}>Cancel sync</Button>
        {:else}
          <Button icon={RefreshCw} variant="ghost" loading={!!app.checkingRepo[name]} onclick={async () => { await checkRepo(name); loadInfo(); }}>Check</Button>
          <Button variant="primary" icon={Download} disabled={!repo.destination} onclick={() => (tab = "sync")}>Synchronize</Button>
        {/if}
        <Menu
          items={[
            ...(repo.destination ? [{ label: "Open download folder", icon: FolderOpen, onclick: () => openPath(repo.destination!) }] : []),
            { label: "Remove repository", icon: Trash2, danger: true, onclick: remove },
          ]}
        />
      {/snippet}
    </PageHeader>

    {#if repo.state.status === "error" && repo.state.last_error}
      <Alert tone="err" title="Last check failed">{repo.state.last_error}</Alert>
    {/if}

    <Tabs
      bind:value={tab}
      param="tab"
      items={[
        { id: "overview", label: "Overview", icon: LayoutDashboard },
        { id: "sync", label: "Synchronize", icon: Download },
        { id: "events", label: "Events", icon: CalendarDays, count: info?.events.length },
        { id: "changelog", label: "Changelog", icon: History },
        { id: "settings", label: "Settings", icon: Settings2 },
        { id: "hosting", label: "Hosting", icon: ServerIcon, hidden: !hosting },
      ]}
    />

    {#if tab === "overview"}
      <Overview {repo} {info} loading={loadingInfo} error={infoError} onretry={loadInfo} />
    {:else if tab === "sync"}
      <SyncTab {repo} {info} />
    {:else if tab === "events"}
      <Events {repo} {info} loading={loadingInfo} error={infoError} onretry={loadInfo} ondownload={(e) => { tab = "sync"; sync(e); }} />
    {:else if tab === "changelog"}
      <Changelog {info} loading={loadingInfo} error={infoError} onretry={loadInfo} />
    {:else if tab === "settings"}
      <RepoSettings {repo} />
    {:else if tab === "hosting"}
      <Hosting {repo} />
    {/if}
  </div>
{/if}
