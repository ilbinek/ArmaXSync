<script lang="ts">
  import { onMount } from "svelte";
  import { Modal, toast } from "@axs/kit";
  import { get, post, put, del } from "../../api";

  let { slug, isAdmin, onjob }: { slug: string; isAdmin: boolean; onjob: (id: number) => void } = $props();
  let creds = $state<any[]>([]);
  let maintainers = $state<string[]>([]);
  let users = $state<any[]>([]);
  let open = $state(false);
  let form = $state({ login: "", password: "", embed: false, note: "" });
  let created: { login: string; password: string } | null = $state(null);

  async function load() {
    creds = await get(`/repos/${slug}/credentials`);
    maintainers = await get(`/repos/${slug}/maintainers`);
    if (isAdmin) users = await get("/users");
  }
  onMount(load);

  async function add() {
    try {
      const r = await post(`/repos/${slug}/credentials`, { ...form, password: form.password || undefined });
      created = { login: r.login, password: r.password };
      open = false;
      form = { login: "", password: "", embed: false, note: "" };
      if (r.job) onjob(r.job);
      load();
    } catch (e: any) {
      toast(e.message, "err");
    }
  }
  async function remove(id: number) {
    if (!confirm("Revoke this login? Players using it lose access.")) return;
    await del(`/repos/${slug}/credentials/${id}`);
    load();
  }
  async function toggleMaintainer(u: string) {
    const next = maintainers.includes(u) ? maintainers.filter((m) => m !== u) : [...maintainers, u];
    await put(`/repos/${slug}/maintainers`, next);
    maintainers = next;
  }
</script>

<div class="stack">
  <div class="card stack">
    <div class="row"><h2 style="margin:0">Download logins</h2><div class="spacer"></div><button class="btn small primary" onclick={() => (open = true)}>Add login</button></div>
    <div class="small muted">Used by players (HTTP Basic auth, compatible with ArmA3Sync) when the repository is private.
      A login marked <b>embedded</b> is published inside the auto-config so players don't have to type it — anyone with the auto-config URL can read it.</div>
    {#if created}
      <div class="alert ok">New login <b>{created.login}</b> / password <code>{created.password}</code> — copy it now, it is not shown again.</div>
    {/if}
    <table>
      <thead><tr><th>Login</th><th>Note</th><th>Embedded</th><th></th></tr></thead>
      <tbody>
        {#each creds as c}
          <tr><td>{c.login}</td><td class="muted">{c.note}</td><td>{c.embed ? "yes" : ""}</td><td style="text-align:right"><button class="btn ghost small danger" onclick={() => remove(c.id)}>Revoke</button></td></tr>
        {:else}<tr><td colspan="4" class="empty">No logins.</td></tr>{/each}
      </tbody>
    </table>
  </div>

  {#if isAdmin}
    <div class="card stack">
      <h2>Maintainers</h2>
      <div class="small muted">Maintainers can upload, publish, edit events and settings of this repository.</div>
      <div class="row">
        {#each users.filter((u) => u.role === "user") as u}
          <label class="check"><input type="checkbox" checked={maintainers.includes(u.username)} onchange={() => toggleMaintainer(u.username)} /> {u.username}</label>
        {:else}<span class="muted small">Create users with the "user" role first (Users page).</span>{/each}
      </div>
    </div>
  {/if}
</div>

<Modal bind:open title="Add download login">
  <label class="field">Login <input bind:value={form.login} /></label>
  <label class="field">Password <span class="hint">Leave empty to generate one.</span><input bind:value={form.password} /></label>
  <label class="field">Note <input bind:value={form.note} placeholder="e.g. clan members" /></label>
  <label class="check"><input type="checkbox" bind:checked={form.embed} /> Embed in the public auto-config (legacy behaviour)</label>
  {#snippet footer()}<button class="btn primary" onclick={add}>Add</button>{/snippet}
</Modal>
