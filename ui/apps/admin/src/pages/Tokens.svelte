<script lang="ts">
  import { onMount } from "svelte";
  import { Modal, date, toast } from "@axs/kit";
  import { get, post, del } from "../api";

  let { isAdmin }: { isAdmin: boolean } = $props();
  let tokens = $state<any[]>([]);
  let repos = $state<any[]>([]);
  let open = $state(false);
  let form = $state({ name: "", scope: "push", repo: "", days: 0 });
  let secret = $state("");

  async function load() {
    tokens = await get("/tokens");
    repos = await get("/repos");
  }
  onMount(load);

  async function create() {
    const scopes = form.scope === "admin" ? ["admin"] : [`${form.scope}:${form.repo}`];
    try {
      const r = await post("/tokens", { name: form.name, scopes, expires_days: form.days || undefined });
      secret = r.token;
      open = false;
      load();
    } catch (e: any) {
      toast(e.message, "err");
    }
  }
  async function revoke(id: number) {
    if (!confirm("Revoke this token?")) return;
    await del(`/tokens/${id}`);
    load();
  }
</script>

<div class="row" style="margin-bottom:14px">
  <div><h1>API tokens</h1><div class="muted">For the desktop client (push), scripts and CI.</div></div>
  <div class="spacer"></div>
  <button class="btn primary" onclick={() => (open = true)}>New token</button>
</div>

{#if secret}
  <div class="alert ok" style="margin-bottom:12px">
    Token created — copy it now, it is shown only once:<br /><code>{secret}</code>
    <div class="small" style="margin-top:6px">Desktop push: <code>armaxsync push --server {location.origin} --server-repo &lt;slug&gt; --token {secret.slice(0, 10)}… --path &lt;folder&gt;</code></div>
  </div>
{/if}

<div class="card flush">
  <table>
    <thead><tr><th>Name</th><th>Scopes</th><th class="hide-sm">Last used</th><th class="hide-sm">Expires</th><th></th></tr></thead>
    <tbody>
      {#each tokens as t}
        <tr>
          <td>{t.name}</td>
          <td><code>{t.scopes.join(" ")}</code></td>
          <td class="hide-sm">{date(t.last_used)}</td>
          <td class="hide-sm">{t.expires_at ? date(t.expires_at) : "never"}</td>
          <td style="text-align:right"><button class="btn ghost small danger" onclick={() => revoke(t.id)}>Revoke</button></td>
        </tr>
      {:else}<tr><td colspan="5" class="empty">No tokens.</td></tr>{/each}
    </tbody>
  </table>
</div>

<Modal bind:open title="New API token">
  <label class="field">Name <input bind:value={form.name} placeholder="e.g. my desktop" /></label>
  <label class="field">Permission
    <select bind:value={form.scope}>
      <option value="push">Push to one repository</option>
      <option value="manage">Manage one repository</option>
      {#if isAdmin}<option value="admin">Full admin</option>{/if}
    </select>
  </label>
  {#if form.scope !== "admin"}
    <label class="field">Repository <select bind:value={form.repo}>{#each repos as r}<option value={r.slug}>{r.name}</option>{/each}</select></label>
  {/if}
  <label class="field">Expires after (days) <span class="hint">0 = never.</span><input type="number" min="0" bind:value={form.days} /></label>
  {#snippet footer()}<button class="btn primary" disabled={!form.name || (form.scope !== "admin" && !form.repo)} onclick={create}>Create</button>{/snippet}
</Modal>
