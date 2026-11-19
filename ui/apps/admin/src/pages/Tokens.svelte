<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Button, DataTable, Badge, Modal, Field, SegmentedControl, Alert, CopyField, ago, date, toastError, errorText, confirmDialog } from "@axs/kit";
  import { KeyRound, Plus, Trash2, Upload, Settings2, Shield } from "@lucide/svelte";
  import { get, post, del, type RepoSummary } from "../api";
  import { isAdmin } from "../session.svelte";

  type T = { id: number; name: string; scopes: string[]; last_used: number | null; expires_at: number | null; created_at?: number };
  let tokens = $state<T[] | null>(null);
  let repos = $state<RepoSummary[]>([]);
  let error = $state<string | null>(null);

  async function load() {
    try {
      [tokens, repos] = await Promise.all([get<T[]>("/tokens"), get<RepoSummary[]>("/repos")]);
      error = null;
    } catch (e) {
      error = errorText(e);
    }
  }
  onMount(load);

  let open = $state(false);
  let form = $state({ name: "", scope: "push" as "push" | "manage" | "admin", repo: "", days: 90 });
  let busy = $state(false);
  let formErr = $state<string | null>(null);
  let created = $state<{ token: string; repo: string; scope: string } | null>(null);

  async function create() {
    busy = true;
    formErr = null;
    try {
      const scopes = form.scope === "admin" ? ["admin"] : [`${form.scope}:${form.repo}`];
      const r = await post<{ token: string }>("/tokens", { name: form.name.trim(), scopes, expires_days: form.days || undefined });
      created = { token: r.token, repo: form.repo, scope: form.scope };
      open = false;
      load();
    } catch (e) {
      formErr = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function revoke(t: T) {
    if (!(await confirmDialog({ title: `Revoke ${t.name}?`, message: "Scripts and apps using it stop working immediately.", confirmLabel: "Revoke", danger: true }))) return;
    try {
      await del(`/tokens/${t.id}`);
      load();
    } catch (e) {
      toastError(e);
    }
  }

  function scopeLabel(s: string): { label: string; icon: any } {
    if (s === "admin") return { label: "Full admin", icon: Shield };
    const [k, repo] = s.split(":");
    const name = repos.find((r) => r.slug === repo)?.name ?? repo;
    return k === "push" ? { label: `Push to ${name}`, icon: Upload } : { label: `Manage ${name}`, icon: Settings2 };
  }
  const expired = (t: T) => !!t.expires_at && t.expires_at < Date.now();
  const columns = [
    { key: "name", label: "Name", sort: (t: T) => t.name.toLowerCase() },
    { key: "scopes", label: "Permission" },
    { key: "used", label: "Last used", sort: (t: T) => -(t.last_used ?? 0), width: "130px", hideSm: true },
    { key: "exp", label: "Expires", sort: (t: T) => t.expires_at ?? Infinity, width: "130px", hideSm: true },
    { key: "actions", label: "", width: "1%" },
  ];
  const cmd = $derived(
    created
      ? `armaxsync push --server ${location.origin} ${created.scope === "admin" ? "--server-repo <slug>" : `--server-repo ${created.repo}`} --token ${created.token} --path <folder>`
      : "",
  );
</script>

<div class="page">
  <PageHeader title="API tokens" subtitle="For pushing from the desktop app or CLI, scripts and CI.">
    {#snippet actions()}<Button variant="primary" icon={Plus} onclick={() => ((open = true), (form = { name: "", scope: "push", repo: repos[0]?.slug ?? "", days: 90 }), (formErr = null))}>New token</Button>{/snippet}
  </PageHeader>

  {#if created}
    <Alert tone="ok" title="Token created" dismissible ondismiss={() => (created = null)}>
      Copy it now: it is shown only once.
      <div class="stack tight" style="margin-top:8px">
        <CopyField label="Token" value={created.token} secret />
        <CopyField label="Push from the command line" value={cmd} hint="In the desktop app: repository → Hosting → API token." />
      </div>
    </Alert>
  {/if}

  <div class="card flush">
    <DataTable rows={tokens} {columns} loading={!tokens} {error} onretry={load} key={(t) => t.id} sortBy="name" emptyIcon={KeyRound} emptyTitle="No API tokens" emptyText="Create one to push to a repository from the desktop app or CLI.">
      {#snippet cell(t, c)}
        {#if c.key === "name"}<span class="strong">{t.name}</span>
        {:else if c.key === "scopes"}
          <span class="row tight">{#each t.scopes as s}{@const l = scopeLabel(s)}<Badge plain icon={l.icon} tone={s === "admin" ? "warn" : "neutral"}>{l.label}</Badge>{/each}</span>
        {:else if c.key === "used"}<span class="small muted" title={t.last_used ? date(t.last_used) : ""}>{t.last_used ? ago(t.last_used) : "Never"}</span>
        {:else if c.key === "exp"}
          {#if expired(t)}<Badge tone="err">Expired</Badge>{:else}<span class="small muted">{t.expires_at ? date(t.expires_at).split(",")[0] : "Never"}</span>{/if}
        {:else if c.key === "actions"}<Button size="sm" variant="ghost" icon={Trash2} onclick={() => revoke(t)}>Revoke</Button>
        {/if}
      {/snippet}
    </DataTable>
  </div>
</div>

<Modal bind:open title="New API token" persistent={busy}>
  <form class="stack" onsubmit={(e) => (e.preventDefault(), create())}>
    <Field label="Name" required hint="Where you will use it.">{#snippet children(id)}<input {id} bind:value={form.name} placeholder="e.g. my desktop" data-autofocus />{/snippet}</Field>
    <Field label="Permission">
      {#snippet children()}
        <SegmentedControl
          bind:value={form.scope}
          options={[{ value: "push", label: "Push", icon: Upload }, { value: "manage", label: "Manage", icon: Settings2 }, ...(isAdmin() ? [{ value: "admin" as const, label: "Full admin", icon: Shield }] : [])]}
        />
      {/snippet}
    </Field>
    <p class="small muted" style="margin:0">
      {form.scope === "push" ? "Upload and publish content to one repository." : form.scope === "manage" ? "Everything a maintainer can do on one repository." : "Everything you can do, on every repository. Keep it safe."}
    </p>
    {#if form.scope !== "admin"}
      <Field label="Repository" required>
        {#snippet children(id)}<select {id} bind:value={form.repo}>{#each repos as r}<option value={r.slug}>{r.name}</option>{/each}</select>{/snippet}
      </Field>
    {/if}
    <Field label="Expires">
      {#snippet children(id)}
        <select {id} bind:value={form.days}>
          <option value={30}>In 30 days</option>
          <option value={90}>In 90 days</option>
          <option value={365}>In a year</option>
          <option value={0}>Never</option>
        </select>
      {/snippet}
    </Field>
    {#if formErr}<Alert tone="err">{formErr}</Alert>{/if}
  </form>
  {#snippet footer()}
    <Button onclick={() => (open = false)} disabled={busy}>Cancel</Button>
    <Button variant="primary" loading={busy} disabled={!form.name.trim() || (form.scope !== "admin" && !form.repo)} onclick={create}>Create token</Button>
  {/snippet}
</Modal>
