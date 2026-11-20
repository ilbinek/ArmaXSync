<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Tabs, Button, Badge, Menu, Section, CopyField, Skeleton, EmptyState, Stat, bytes, date, ago, navigate, toast, toastError, confirmWithOption, errorText } from "@axs/kit";
  import { Database, LayoutDashboard, ChartColumn, FolderTree, CalendarDays, Settings2, ShieldCheck, Timer, ListChecks, Hammer, FolderSync, ExternalLink, Trash2, Lock, Globe, CircleAlert, RotateCw, Share2, LoaderCircle } from "@lucide/svelte";
  import { get, post, del, type RepoDetail } from "../api";
  import { session, isAdmin, openJob } from "../session.svelte";
  import Files from "./repo/Files.svelte";
  import Events from "./repo/Events.svelte";
  import Settings from "./repo/Settings.svelte";
  import Access from "./repo/Access.svelte";
  import Automation from "./repo/Automation.svelte";
  import JobList from "../components/JobList.svelte";


  let { slug }: { slug: string } = $props();
  let repo = $state<RepoDetail | null>(null);
  let error = $state<string | null>(null);
  let tab = $state<string>();

  async function reload() {
    try {
      repo = await get<RepoDetail>(`/repos/${encodeURIComponent(slug)}`);
      error = null;
    } catch (e) {
      error = errorText(e);
    }
  }
  onMount(reload);
  // Any job finishing (even with the drawer closed) can change the repository.
  let seen = session.jobsDone;
  $effect(() => {
    if (session.jobsDone !== seen) {
      seen = session.jobsDone;
      reload();
    }
  });

  const running = $derived(session.active.filter((j) => j.repo === slug));

  async function runJob(kind: "build" | "mirror") {
    try {
      const r = await post<{ job: number }>(`/repos/${encodeURIComponent(slug)}/${kind}`);
      openJob(r.job);
    } catch (e) {
      toastError(e);
    }
  }

  async function remove() {
    if (!repo) return;
    const c = await confirmWithOption({
      title: `Delete ${repo.name}?`,
      message: "Players can no longer sync it. This cannot be undone.",
      checkbox: repo.mode === "managed" ? { label: "Also delete its files from the server", checked: false } : undefined,
      confirmLabel: "Delete repository",
      danger: true,
    });
    if (!c) return;
    try {
      await del(`/repos/${encodeURIComponent(slug)}?delete_files=${repo.mode === "managed" && c.checked}`);
      toast(`${repo.name} deleted`);
      navigate("/repos");
    } catch (e) {
      toastError(e);
    }
  }
</script>

{#if error && !repo}
  <div class="card">
    <EmptyState icon={CircleAlert} title="Could not load the repository" description={error}>
      <Button icon={RotateCw} onclick={reload}>Retry</Button>
      <Button href="#/repos">All repositories</Button>
    </EmptyState>
  </div>
{:else if !repo}
  <div class="page"><Skeleton block height={70} /><Skeleton lines={6} /></div>
{:else}
  <div class="page">
    <PageHeader title={repo.name} crumbs={[{ label: "Repositories", href: "#/repos" }]} icon={Database}>
      {#snippet meta()}
        <span class="mono">/r/{repo!.slug}</span>
        <span>Revision {repo!.revision}</span>
        <span>{repo!.files.toLocaleString()} files · {bytes(repo!.size)}</span>
        {#if repo!.visibility === "private"}<Badge plain icon={Lock}>Private</Badge>{:else}<Badge plain icon={Globe}>Public</Badge>{/if}
        {#if running.length}<button class="linkish" onclick={() => openJob(running[0].id)}><Badge tone="accent" dot pulse>Job running</Badge></button>{/if}
      {/snippet}
      {#snippet actions()}
        {#if repo!.has_mirror}<Button icon={FolderSync} disabled={running.length > 0} onclick={() => runJob("mirror")}>Pull from mirror</Button>{/if}
        <Button icon={running.length ? LoaderCircle : Hammer} disabled={running.length > 0} tip={running.length ? "A job is running for this repository" : "Re-scan the folder and publish a new revision"} onclick={() => runJob("build")}>Rebuild</Button>
        <Menu
          items={[
            { label: "Public page", icon: ExternalLink, onclick: () => window.open(`/repo/${repo!.slug}`, "_blank", "noopener") },
            ...(isAdmin() ? [{ divider: true as const }, { label: "Delete repository", icon: Trash2, danger: true, onclick: remove }] : []),
          ]}
        />
      {/snippet}
    </PageHeader>

    <Tabs
      bind:value={tab}
      param="tab"
      items={[
        { id: "overview", label: "Overview", icon: LayoutDashboard },
        { id: "stats", label: "Stats", icon: ChartColumn },
        { id: "files", label: "Files", icon: FolderTree },
        { id: "events", label: "Events", icon: CalendarDays, count: repo.settings.events.length },
        { id: "settings", label: "Settings", icon: Settings2 },
        { id: "access", label: "Access", icon: ShieldCheck },
        { id: "automation", label: "Mirror & schedules", icon: Timer },
        { id: "jobs", label: "Jobs", icon: ListChecks, count: running.length || null },
      ]}
    />

    {#if tab === "overview"}
      <div class="grid3">
        <Stat label="Revision" value={repo.revision} hint={repo.build_date ? `Published ${ago(repo.build_date)}` : "Never published"} />
        <Stat label="Content" value={bytes(repo.size)} hint={`${repo.files.toLocaleString()} files`} />
        <Stat label="Served" value={bytes(repo.bytes_served)} hint={`${repo.requests.toLocaleString()} requests since start`} />
      </div>
      <div class="cols">
        <Section title="Share with players" icon={Share2} description="Send players one of these. The desktop app opens armaxsync:// links directly.">
          <CopyField label="One-click link (ArmaXSync)" value={repo.deep_link} />
          <CopyField label="Auto-config URL (ArmA3Sync and ArmaXSync)" value={repo.autoconfig_url} />
          <CopyField label="Repository URL" value={repo.url} />
          <CopyField label="Public page" value={`${location.origin}/repo/${repo.slug}`} href={`/repo/${repo.slug}`} hint={repo.settings.listed ? "Listed on the public landing page." : "Not listed on the landing page, but reachable by link."} />
        </Section>
        <div class="stack">
          <Section title="Details">
            <div class="kv">
              <span>Game</span><span>{repo.game}</span>
              <span>Content</span><span>{repo.mode === "external" ? "Existing folder" : "Managed by the server"}</span>
              <span>Folder</span><span><code>{repo.path}</code></span>
              <span>Created</span><span>{date(repo.created_at)}</span>
              <span>Last publish</span><span>{date(repo.build_date)}</span>
            </div>
          </Section>
          <Section title="Recent jobs" flush>
            {#snippet actions()}<Button size="sm" variant="ghost" onclick={() => (tab = "jobs")}>All</Button>{/snippet}
            <JobList repo={slug} limit={5} compact />
          </Section>
        </div>
      </div>
    {:else if tab === "stats"}
      {#await import("../components/stats/StatsView.svelte")}
        <Skeleton block height={320} />
      {:then m}
        <m.default repo={slug} />
      {/await}
    {:else if tab === "files"}
      <Files {repo} />
    {:else if tab === "events"}
      <Events {slug} />
    {:else if tab === "settings"}
      <Settings {repo} onsaved={reload} />
    {:else if tab === "access"}
      <Access {repo} />
    {:else if tab === "automation"}
      <Automation {repo} onsaved={reload} />
    {:else if tab === "jobs"}
      <div class="card flush"><JobList repo={slug} /></div>
    {/if}
  </div>
{/if}

<style>
  .cols { display: grid; grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr); gap: 16px; align-items: start; }
  @media (max-width: 1100px) { .cols { grid-template-columns: 1fr; } }
  .linkish { background: none; border: 0; padding: 0; cursor: pointer; }
</style>
