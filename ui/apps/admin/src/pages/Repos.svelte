<script lang="ts">
  import { onMount } from "svelte";
  import { Modal, bytes, ago, navigate, toast } from "@axs/kit";
  import { get, post } from "../api";

  let { isAdmin }: { isAdmin: boolean } = $props();
  let repos = $state<any[]>([]);
  let loading = $state(true);
  let open = $state(false);
  let form = $state({ slug: "", name: "", description: "", game: "arma3", mode: "managed", path: "", mirror: "" });
  let error = $state("");

  async function load() {
    repos = await get("/repos");
    loading = false;
  }
  onMount(load);

  function slugify() {
    if (!form.slug) form.slug = form.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  }

  async function create(e: Event) {
    e.preventDefault();
    error = "";
    try {
      const settings: any = {};
      if (form.mirror.trim()) settings.mirror = { url: form.mirror.trim(), import_events: true, import_hidden: true };
      await post("/repos", {
        slug: form.slug,
        name: form.name,
        description: form.description,
        game: form.game,
        mode: form.mode,
        path: form.mode === "external" ? form.path : undefined,
        settings: Object.keys(settings).length ? { ...defaults(), ...settings } : undefined,
      });
      open = false;
      toast(`Repository ${form.name} created`);
      navigate(`/repos/${form.slug}`);
    } catch (err: any) {
      error = err.message;
    }
  }

  function defaults() {
    return { visibility: "public", compress: false, compressed_only: false, connections: 3, partial_transfer: true, embed_credentials: true, listed: true };
  }
</script>

<div class="row" style="margin-bottom:14px">
  <div>
    <h1>Repositories</h1>
    <div class="muted">Mod repositories hosted by this server.</div>
  </div>
  <div class="spacer"></div>
  {#if isAdmin}<button class="btn primary" onclick={() => (open = true)}>New repository</button>{/if}
</div>

{#if loading}
  <div class="empty">Loading…</div>
{:else if repos.length === 0}
  <div class="card empty">
    No repositories yet.
    {#if isAdmin}<div style="margin-top:10px"><button class="btn primary" onclick={() => (open = true)}>Create the first repository</button></div>{/if}
  </div>
{:else}
  <div class="card flush">
    <table>
      <thead><tr><th>Name</th><th>Revision</th><th class="hide-sm">Files</th><th>Size</th><th class="hide-sm">Updated</th><th class="hide-sm">Served</th></tr></thead>
      <tbody>
        {#each repos as r}
          <tr class="clickable" onclick={() => navigate(`/repos/${r.slug}`)}>
            <td>
              <b>{r.name}</b>
              <div class="small muted">{r.slug} · {r.game}{r.mode === "external" ? " · external folder" : ""}{r.has_mirror ? " · mirror" : ""}</div>
            </td>
            <td>{r.revision || "-"}</td>
            <td class="hide-sm">{r.files}</td>
            <td>{bytes(r.size)}</td>
            <td class="hide-sm">{ago(r.build_date)}</td>
            <td class="hide-sm">{bytes(r.bytes_served)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<Modal bind:open title="New repository">
  <form id="newrepo" class="stack" onsubmit={create}>
    <label class="field">Name <input bind:value={form.name} onblur={slugify} required /></label>
    <label class="field">Slug <span class="hint">Used in URLs: /r/&lt;slug&gt;</span><input bind:value={form.slug} pattern="[A-Za-z0-9_-]+" required /></label>
    <label class="field">Description <input bind:value={form.description} /></label>
    <div class="grid2">
      <label class="field">Game
        <select bind:value={form.game}><option value="arma3">Arma 3</option><option value="reforger">Arma Reforger</option></select>
      </label>
      <label class="field">Content
        <select bind:value={form.mode}>
          <option value="managed">Managed by the server</option>
          <option value="external">Existing folder on the server</option>
        </select>
      </label>
    </div>
    {#if form.mode === "external"}
      <label class="field">Folder <span class="hint">Absolute path on the server; edit it over SFTP/SMB and click Build.</span><input bind:value={form.path} required /></label>
    {:else}
      <label class="field">Mirror from (optional)
        <span class="hint">Autoconfig or base URL of another Arma3Sync/ArmaXSync repository to pull from.</span>
        <input bind:value={form.mirror} placeholder="https://example.com/repo/.a3s/autoconfig" />
      </label>
    {/if}
    {#if error}<div class="alert err">{error}</div>{/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={() => (open = false)}>Cancel</button>
    <button class="btn primary" form="newrepo">Create</button>
  {/snippet}
</Modal>
