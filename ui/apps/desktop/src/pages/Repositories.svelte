<script lang="ts">
  import { PageHeader, Button, DataTable, Menu, EmptyState, Progress, Badge, ago, bytes, speed, duration, navigate, confirmDialog, toastError } from "@axs/kit";
  import { Plus, RefreshCw, Download, X, Database, FolderOpen, Settings2, Trash2, Search } from "@lucide/svelte";
  import { call, openPath, type RepoConfig } from "../api";
  import { app, checkAll, checkRepo, startSync, cancelSync, refresh } from "../store.svelte";

  let { onadd }: { onadd: () => void } = $props();

  const repos = $derived(app.state?.repos ?? []);
  const open = (r: RepoConfig) => navigate(`/repos/${encodeURIComponent(r.name)}`);

  const status = (r: RepoConfig) =>
    app.progress[r.name]
      ? { tone: "accent" as const, label: "Syncing", pulse: true }
      : r.state.status === "updated"
        ? { tone: "warn" as const, label: "Update" }
        : r.state.status === "ok"
          ? { tone: "ok" as const, label: "Up to date" }
          : r.state.status === "error"
            ? { tone: "err" as const, label: "Error" }
            : { tone: "neutral" as const, label: "Not checked" };

  async function sync(r: RepoConfig) {
    if (!r.destination) return open(r);
    try {
      await startSync(r.name);
    } catch (e) {
      toastError(e, r.name);
    }
  }

  async function remove(r: RepoConfig) {
    if (!(await confirmDialog({ title: `Remove ${r.name}?`, message: "The repository is removed from ArmaXSync. Downloaded files stay on disk.", confirmLabel: "Remove", danger: true }))) return;
    try {
      await call("repo_remove", { name: r.name });
      await refresh();
    } catch (e) {
      toastError(e);
    }
  }

  const columns = [
    { key: "name", label: "Repository", sort: (r: RepoConfig) => r.name.toLowerCase() },
    { key: "status", label: "Status", sort: (r: RepoConfig) => ({ updated: 0, error: 1, unknown: 2, ok: 3 })[r.state.status], width: "130px" },
    { key: "rev", label: "Revision", align: "right" as const, width: "110px", hideSm: true },
    { key: "sync", label: "Last sync", sort: (r: RepoConfig) => -(r.state.last_sync ?? 0), width: "120px", hideSm: true },
    { key: "actions", label: "", align: "right" as const, width: "1%" },
  ];
</script>

<div class="page">
  <PageHeader title="Repositories" subtitle="Mod repositories you download from. ArmaXSync checks them for updates in the background.">
    {#snippet actions()}
      {#if repos.length}<Button icon={RefreshCw} loading={app.checking} onclick={() => checkAll(false)}>Check all</Button>{/if}
      <Button variant="primary" icon={Plus} onclick={onadd}>Add repository</Button>
    {/snippet}
  </PageHeader>

  {#if !repos.length}
    <div class="card">
      <EmptyState icon={Database} title="No repositories yet" description="Your community gives you an auto-config URL or an armaxsync:// link. Add it here to download its mods and keep them up to date.">
        <Button variant="primary" icon={Plus} onclick={onadd}>Add repository</Button>
      </EmptyState>
    </div>
  {:else}
    <div class="card flush">
      <DataTable
        rows={repos}
        {columns}
        key={(r) => r.name}
        onrowclick={open}
        sortBy="status"
        search={repos.length > 5 ? (r, q) => r.name.toLowerCase().includes(q) || r.url.toLowerCase().includes(q) : undefined}
        searchPlaceholder="Filter repositories…"
      >
        {#snippet cell(r, c)}
          {@const p = app.progress[r.name]}
          {#if c.key === "name"}
            <div class="name">
              <span class="strong">{r.name}</span>
              {#if p}
                <div class="prog">
                  <Progress size="sm" value={p.bytes_done} max={p.bytes_total} showPct={false} />
                  <span class="xs muted tabular">
                    {bytes(p.bytes_done)} / {bytes(p.bytes_total)} · {speed(p.speed)}{p.speed > 0 ? ` · ${duration((p.bytes_total - p.bytes_done) / p.speed)} left` : ""}{#if p.failed}<span class="err-text"> · {p.failed} failed</span>{/if}
                  </span>
                </div>
              {:else if r.state.status === "error" && r.state.last_error}
                <span class="xs err-text ellipsis" title={r.state.last_error}>{r.state.last_error}</span>
              {:else}
                <span class="xs subtle mono ellipsis" title={r.destination ?? ""}>{r.destination ?? "No download folder set"}</span>
              {/if}
            </div>
          {:else if c.key === "status"}
            {@const s = status(r)}
            <Badge tone={s.tone} dot pulse={s.pulse}>{s.label}</Badge>
          {:else if c.key === "rev"}
            <span class="tabular">
              {r.state.revision ?? "–"}{#if r.state.remote_revision != null && r.state.remote_revision !== r.state.revision}<span class="warn-text">&nbsp;→ {r.state.remote_revision}</span>{/if}
            </span>
          {:else if c.key === "sync"}
            <span class="small muted" title={r.state.last_check ? `Checked ${ago(r.state.last_check)}` : ""}>{ago(r.state.last_sync)}</span>
          {:else if c.key === "actions"}
            <div class="row nowrap tight">
              {#if p}
                <Button size="sm" icon={X} onclick={() => cancelSync(r.name)}>Cancel</Button>
              {:else}
                <Button size="sm" variant="ghost" icon={RefreshCw} tip="Check for updates" loading={!!app.checkingRepo[r.name]} onclick={() => checkRepo(r.name)} />
                <Button size="sm" variant={r.state.status === "updated" ? "primary" : "default"} icon={Download} onclick={() => sync(r)}>Sync</Button>
              {/if}
              <Menu
                items={[
                  { label: "Open", icon: Search, onclick: () => open(r) },
                  { label: "Settings", icon: Settings2, onclick: () => navigate(`/repos/${encodeURIComponent(r.name)}?tab=settings`) },
                  ...(r.destination ? [{ label: "Open download folder", icon: FolderOpen, onclick: () => openPath(r.destination!) }] : []),
                  { divider: true as const },
                  { label: "Remove", icon: Trash2, danger: true, onclick: () => remove(r) },
                ]}
              />
            </div>
          {/if}
        {/snippet}
      </DataTable>
    </div>
  {/if}
</div>

<style>
  .name { display: flex; flex-direction: column; gap: 3px; min-width: 0; max-width: 520px; }
  .prog { display: flex; flex-direction: column; gap: 4px; max-width: 360px; }
</style>
