<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Button, DataTable, Badge, ago, date, errorText } from "@axs/kit";
  import { ScrollText, X } from "@lucide/svelte";
  import { get } from "../api";

  type Row = { id: number; at: number; username: string | null; action: string; target: string | null; details: Record<string, unknown> | null; ip: string | null };
  let rows = $state<Row[] | null>(null);
  let error = $state<string | null>(null);
  let more = $state(false);
  let user = $state("");
  let action = $state("");
  let target = $state("");

  const labels: Record<string, string> = {
    login: "Signed in",
    "login.failed": "Failed sign-in",
    "setup.owner_created": "Owner created",
    "repo.create": "Created repository",
    "repo.update": "Changed repository settings",
    "repo.delete": "Deleted repository",
    "repo.build": "Started a rebuild",
    "repo.mirror": "Started a mirror pull",
    "repo.events.update": "Changed events",
    "repo.schedules.update": "Changed schedules",
    "repo.maintainers.update": "Changed maintainers",
    "repo.credential.add": "Added a download login",
    "repo.credential.delete": "Revoked a download login",
    "changeset.commit": "Published changes",
    "job.cancel": "Cancelled a job",
    "token.create": "Created an API token",
    "token.delete": "Revoked an API token",
    "user.create": "Created a user",
    "user.update": "Changed a user",
    "user.delete": "Deleted a user",
    "server.update": "Started a server update",
  };
  const tone = (a: string) => (a === "login.failed" ? "err" : a.endsWith(".delete") ? "warn" : a.startsWith("server") ? "accent" : "neutral");

  async function load(append = false) {
    error = null;
    try {
      const q = new URLSearchParams({ limit: "100" });
      if (append && rows?.length) q.set("before", String(rows[rows.length - 1].id));
      if (user) q.set("user", user);
      if (action) q.set("action", action);
      if (target) q.set("target", target);
      const page = await get<Row[]>(`/audit?${q}`);
      rows = append && rows ? [...rows, ...page] : page;
      more = page.length === 100;
    } catch (e) {
      error = errorText(e);
    }
  }
  onMount(() => load());
  const filtered = $derived(!!(user || action || target));

  const columns = [
    { key: "when", label: "When", width: "130px" },
    { key: "who", label: "Who", width: "140px" },
    { key: "what", label: "What" },
    { key: "ip", label: "IP", width: "130px", hideSm: true },
  ];
</script>

<div class="page">
  <PageHeader title="Audit log" subtitle="Administrative actions on this server, newest first." />
  <div class="card flush">
    <DataTable rows={rows} {columns} loading={!rows} {error} onretry={() => load()} key={(r) => r.id} emptyIcon={ScrollText} emptyTitle={filtered ? "No matching entries" : "Nothing logged yet"}>
      {#snippet toolbar()}
        <select bind:value={action} onchange={() => load()} style="width:auto" aria-label="Action">
          <option value="">All actions</option>
          <option value="login">Sign-ins</option>
          <option value="repo.">Repositories</option>
          <option value="changeset">Publishes</option>
          <option value="user.">Users</option>
          <option value="token.">API tokens</option>
          <option value="server.">Server</option>
        </select>
        <input placeholder="User" bind:value={user} onchange={() => load()} style="width:140px" aria-label="User" />
        <input placeholder="Target (e.g. repository)" bind:value={target} onchange={() => load()} style="width:200px" aria-label="Target" />
        {#if filtered}<Button size="sm" variant="ghost" icon={X} onclick={() => ((user = action = target = ""), load())}>Clear</Button>{/if}
      {/snippet}
      {#snippet cell(r, c)}
        {#if c.key === "when"}<span class="small muted" title={date(r.at)}>{ago(r.at)}</span>
        {:else if c.key === "who"}<span class="small">{r.username ?? "–"}</span>
        {:else if c.key === "what"}
          <div class="stack" style="gap:3px">
            <span class="row tight">
              <Badge plain tone={tone(r.action)}>{labels[r.action] ?? r.action}</Badge>
              {#if r.target}<button class="linkish mono small" onclick={() => ((target = r.target!), load())} title="Filter by this target">{r.target}</button>{/if}
            </span>
            {#if r.details && Object.keys(r.details).length}
              <span class="details xs muted">{#each Object.entries(r.details) as [k, v]}<span><b>{k}</b> {typeof v === "object" ? JSON.stringify(v) : String(v)}</span>{/each}</span>
            {/if}
          </div>
        {:else if c.key === "ip"}<span class="mono xs muted">{r.ip ?? ""}</span>
        {/if}
      {/snippet}
    </DataTable>
    {#if more}<div class="row" style="justify-content:center;padding:10px"><Button size="sm" onclick={() => load(true)}>Older entries</Button></div>{/if}
  </div>
</div>

<style>
  .linkish { background: none; border: 0; padding: 0; color: var(--accent-text); cursor: pointer; }
  .details { display: flex; flex-wrap: wrap; gap: 4px 12px; }
  .details b { color: var(--fg-3); font-weight: 600; }
</style>
