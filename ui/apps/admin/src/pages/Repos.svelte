<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Button, DataTable, Badge, bytes, ago, navigate, errorText } from "@axs/kit";
  import { Plus, Database, Lock, Globe, FolderSync, HardDrive } from "@lucide/svelte";
  import { get, type RepoSummary } from "../api";
  import { isAdmin } from "../session.svelte";
  import CreateRepo from "../components/CreateRepo.svelte";

  let repos = $state<RepoSummary[] | null>(null);
  let error = $state<string | null>(null);
  let createOpen = $state(false);

  async function load() {
    error = null;
    try {
      repos = await get<RepoSummary[]>("/repos");
    } catch (e) {
      error = errorText(e);
    }
  }
  onMount(load);

  const columns = [
    { key: "name", label: "Repository", sort: (r: RepoSummary) => r.name.toLowerCase() },
    { key: "rev", label: "Revision", sort: (r: RepoSummary) => r.revision, align: "right" as const, width: "90px" },
    { key: "size", label: "Size", sort: (r: RepoSummary) => r.size, align: "right" as const, width: "110px", hideSm: true },
    { key: "updated", label: "Published", sort: (r: RepoSummary) => -r.build_date, width: "130px", hideSm: true },
    { key: "served", label: "Served", sort: (r: RepoSummary) => -r.bytes_served, align: "right" as const, width: "110px", hideSm: true },
    { key: "vis", label: "Access", width: "110px" },
  ];
</script>

<div class="page">
  <PageHeader title="Repositories" subtitle="Repositories hosted on this server.">
    {#snippet actions()}{#if isAdmin()}<Button variant="primary" icon={Plus} onclick={() => (createOpen = true)}>New repository</Button>{/if}{/snippet}
  </PageHeader>
  <div class="card flush">
    <DataTable
      rows={repos}
      {columns}
      loading={!repos}
      {error}
      onretry={load}
      key={(r) => r.slug}
      onrowclick={(r) => navigate(`/repos/${encodeURIComponent(r.slug)}`)}
      sortBy="name"
      search={(r, q) => r.name.toLowerCase().includes(q) || r.slug.includes(q) || r.description.toLowerCase().includes(q)}
      searchPlaceholder="Search repositories…"
      emptyIcon={Database}
      emptyTitle="No repositories yet"
      emptyText="Create one, then upload mods, push from the desktop app or mirror another repository."
    >
      {#snippet cell(r, c)}
        {#if c.key === "name"}
          <div class="stack" style="gap:2px">
            <span class="strong">{r.name}</span>
            <span class="xs muted row tight">
              <span class="mono">/r/{r.slug}</span>
              {#if r.mode === "external"}<span class="row tight"><HardDrive size={12} />external folder</span>{/if}
              {#if r.has_mirror}<span class="row tight"><FolderSync size={12} />mirror</span>{/if}
            </span>
          </div>
        {:else if c.key === "rev"}<span class="tabular">{r.revision}</span>
        {:else if c.key === "size"}<span class="tabular">{bytes(r.size)}</span>
        {:else if c.key === "updated"}<span class="small muted">{r.build_date ? ago(r.build_date) : "never"}</span>
        {:else if c.key === "served"}<span class="tabular small" title="Since the server started">{bytes(r.bytes_served)}</span>
        {:else if c.key === "vis"}
          {#if r.visibility === "private"}<Badge plain icon={Lock}>Private</Badge>{:else}<Badge plain icon={Globe}>Public</Badge>{/if}
        {/if}
      {/snippet}
    </DataTable>
  </div>
</div>

<CreateRepo bind:open={createOpen} />
