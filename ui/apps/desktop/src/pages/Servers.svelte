<script lang="ts">
  import { PageHeader, Button, Drawer, Field, PasswordInput, EmptyState, Badge, Menu, confirmDialog, toast, toastError, navigate, copyText } from "@axs/kit";
  import { Server as ServerIcon, Plus, Crosshair, Pencil, Trash2, Lock, Database, Copy } from "@lucide/svelte";
  import type { FavoriteServer } from "../api";
  import { app, saveConfig } from "../store.svelte";

  const servers = $derived(app.state?.config.favorite_servers ?? []);
  const modsetChoices = $derived([
    ...new Set([
      ...(app.profile?.groups.filter((g) => g.type === "group").map((g) => g.name) ?? []),
      ...(app.state?.repos.map((r) => r.name) ?? []),
    ]),
  ]);

  let open = $state(false);
  let editing = $state<number | null>(null);
  let form = $state<FavoriteServer>({ name: "", host: "", port: 2302, password: null, modset: null, repository: null });
  let touched = $state(false);
  let busy = $state(false);

  const errors = $derived({
    name: !form.name.trim() ? "Required" : servers.some((s, i) => i !== editing && s.name.toLowerCase() === form.name.trim().toLowerCase()) ? "Another server has this name" : null,
    host: !form.host.trim() ? "Required" : /\s/.test(form.host.trim()) ? "No spaces" : null,
    port: !(form.port >= 1 && form.port <= 65535) ? "1–65535" : null,
  });
  const valid = $derived(!errors.name && !errors.host && !errors.port);

  function edit(i: number | null) {
    editing = i;
    touched = false;
    form = i == null ? { name: "", host: "", port: 2302, password: null, modset: null, repository: null } : structuredClone($state.snapshot(servers[i]));
    open = true;
  }

  async function save() {
    touched = true;
    if (!valid) return;
    busy = true;
    const s = { ...$state.snapshot(form), name: form.name.trim(), host: form.host.trim(), password: form.password || null, modset: form.modset || null };
    const list = [...$state.snapshot(servers)];
    if (editing == null) list.push(s);
    else list[editing] = s;
    try {
      await saveConfig({ favorite_servers: list });
      open = false;
      toast(editing == null ? `${s.name} added` : `${s.name} saved`);
    } catch (e) {
      toastError(e);
    } finally {
      busy = false;
    }
  }

  async function remove(i: number) {
    const s = servers[i];
    if (!(await confirmDialog({ title: `Remove ${s.name}?`, confirmLabel: "Remove", danger: true }))) return;
    try {
      await saveConfig({ favorite_servers: servers.filter((_, j) => j !== i) });
    } catch (e) {
      toastError(e);
    }
  }

  function join(s: FavoriteServer) {
    app.joinServer = s.name;
    navigate("/");
  }
</script>

<div class="page">
  <PageHeader title="Servers" subtitle="Favorite game servers. Joining one can switch your modset automatically.">
    {#snippet actions()}<Button variant="primary" icon={Plus} onclick={() => edit(null)}>Add server</Button>{/snippet}
  </PageHeader>

  {#if !servers.length}
    <div class="card">
      <EmptyState icon={ServerIcon} title="No servers yet" description="Add a server by address, or add a repository: servers it publishes appear in its Overview tab.">
        <Button variant="primary" icon={Plus} onclick={() => edit(null)}>Add server</Button>
      </EmptyState>
    </div>
  {:else}
    <div class="grid">
      {#each servers as s, i (s.name)}
        <article class="card srv" class:active={app.profile?.server === s.name}>
          <div class="row nowrap top">
            <div class="ic"><ServerIcon size={18} /></div>
            <div class="grow" style="min-width:0">
              <h2 class="ellipsis">{s.name}</h2>
              <div class="mono small muted ellipsis">{s.host}:{s.port}</div>
            </div>
            <Menu
              items={[
                { label: "Edit", icon: Pencil, onclick: () => edit(i) },
                { label: "Copy address", icon: Copy, onclick: async () => (await copyText(`${s.host}:${s.port}`)) && toast("Address copied") },
                { divider: true },
                { label: "Remove", icon: Trash2, danger: true, onclick: () => remove(i) },
              ]}
            />
          </div>
          <div class="row tight meta">
            {#if s.modset}<Badge tone="info" plain>Modset: {s.modset}</Badge>{:else}<Badge plain>Current modset</Badge>{/if}
            {#if s.password}<Badge plain icon={Lock}>Password</Badge>{/if}
            {#if s.repository}<Badge plain icon={Database}>{s.repository}</Badge>{/if}
          </div>
          <div class="row end">
            {#if app.profile?.server === s.name}<span class="xs accent-text grow">Selected on Play</span>{/if}
            <Button size="sm" variant="primary" icon={Crosshair} onclick={() => join(s)}>Join</Button>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

<Drawer bind:open title={editing == null ? "Add server" : `Edit ${servers[editing]?.name ?? "server"}`} width={460} persistent={busy}>
  <form class="stack" onsubmit={(e) => (e.preventDefault(), save())}>
    <Field label="Name" required error={touched ? errors.name : null}>
      {#snippet children(id)}<input {id} bind:value={form.name} placeholder="Main server" />{/snippet}
    </Field>
    <div class="hostport">
      <Field label="Address" required error={touched ? errors.host : null}>
        {#snippet children(id)}<input {id} class="mono" bind:value={form.host} placeholder="play.example.com" spellcheck="false" />{/snippet}
      </Field>
      <Field label="Port" required error={touched ? errors.port : null}>
        {#snippet children(id)}<input {id} type="number" min="1" max="65535" bind:value={form.port} />{/snippet}
      </Field>
    </div>
    <Field label="Password" hint="Leave empty for public servers.">
      {#snippet children(id)}<PasswordInput {id} bind:value={form.password} />{/snippet}
    </Field>
    <Field label="Join with modset" hint="Selecting this server on Play switches to these addons.">
      {#snippet children(id)}
        <select {id} bind:value={form.modset}>
          <option value={null}>Keep my current selection</option>
          {#each modsetChoices as m}<option value={m}>{m}</option>{/each}
          {#if form.modset && !modsetChoices.includes(form.modset)}<option value={form.modset}>{form.modset} (not in this profile)</option>{/if}
        </select>
      {/snippet}
    </Field>
  </form>
  {#snippet footer()}
    <Button onclick={() => (open = false)} disabled={busy}>Cancel</Button>
    <Button variant="primary" loading={busy} onclick={save}>{editing == null ? "Add server" : "Save"}</Button>
  {/snippet}
</Drawer>

<style>
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 14px; }
  .srv { display: flex; flex-direction: column; gap: 12px; }
  .srv.active { border-color: var(--accent-line); }
  .srv h2 { margin: 0 0 2px; }
  .ic { width: 38px; height: 38px; border-radius: var(--r); display: grid; place-items: center; background: var(--surface-3); color: var(--accent-text); flex-shrink: 0; }
  .meta { min-height: 21px; }
  .hostport { display: grid; grid-template-columns: 1fr 110px; gap: 12px; }
</style>
