<script lang="ts">
  // Form state is initialised once from the props (the page re-mounts after saving).
  // svelte-ignore state_referenced_locally
  import { toast } from "@axs/kit";
  import { patch } from "../../api";

  let { repo, onsaved }: { repo: any; onsaved: (job: number | null) => void } = $props();
  let name = $state(repo.name);
  let description = $state(repo.description);
  let s = $state(structuredClone(repo.settings));
  let hidden = $state((repo.settings.hidden ?? []).join("\n"));
  let exclude = $state((repo.settings.exclude ?? []).join("\n"));
  let servers = $state(structuredClone(repo.settings.servers ?? []));
  let mbps = $state(repo.settings.bandwidth_limit ? repo.settings.bandwidth_limit / 1e6 : 0);
  let busy = $state(false);

  async function save() {
    busy = true;
    try {
      const settings = {
        ...s,
        hidden: hidden.split("\n").map((x: string) => x.trim()).filter(Boolean),
        exclude: exclude.split("\n").map((x: string) => x.trim()).filter(Boolean),
        servers: servers.filter((x: any) => x.name && x.host),
        bandwidth_limit: Math.round((Number(mbps) || 0) * 1e6),
      };
      const r = await patch(`/repos/${repo.slug}`, { name, description, settings });
      toast(r.job ? "Saved — publishing metadata" : "Saved");
      onsaved(r.job ?? null);
    } catch (e: any) {
      toast(e.message, "err");
    } finally {
      busy = false;
    }
  }
</script>

<div class="stack">
  <div class="card stack">
    <h2>General</h2>
    <div class="grid2">
      <label class="field">Name <input bind:value={name} /></label>
      <label class="field">Description <input bind:value={description} /></label>
    </div>
    <div class="grid2">
      <label class="field">Visibility
        <select bind:value={s.visibility}>
          <option value="public">Public — anyone with the URL</option>
          <option value="private">Private — login required (see Access)</option>
        </select>
      </label>
      <label class="check"><input type="checkbox" bind:checked={s.listed} /> Show on the public landing page</label>
    </div>
  </div>

  <div class="card stack">
    <h2>Downloads</h2>
    <div class="grid2">
      <label class="field">Connections per client <span class="hint">Advertised to clients (1–10).</span><input type="number" min="1" max="10" bind:value={s.connections} /></label>
      <label class="field">Bandwidth limit (MB/s) <span class="hint">0 = unlimited.</span><input type="number" min="0" step="0.5" bind:value={mbps} /></label>
      <label class="field">Concurrent downloads <span class="hint">0 = unlimited.</span><input type="number" min="0" bind:value={s.max_downloads} /></label>
    </div>
    <label class="check"><input type="checkbox" bind:checked={s.partial_transfer} /> Partial transfers (delta updates and resume)</label>
    <label class="check"><input type="checkbox" bind:checked={s.compress} /> Provide compressed PBOs (.pbo.zip)</label>
    {#if s.compress}<label class="check"><input type="checkbox" bind:checked={s.compressed_only} /> Clients download only compressed PBOs</label>{/if}
  </div>

  <div class="card stack">
    <h2>Content rules</h2>
    <div class="grid2">
      <label class="field">Folders clients must not clean up <span class="hint">One per line (e.g. userconfig). Players' extra files there are kept.</span><textarea bind:value={hidden}></textarea></label>
      <label class="field">Excluded files <span class="hint">Relative paths left out of the repository.</span><textarea bind:value={exclude}></textarea></label>
    </div>
  </div>

  <div class="card stack">
    <h2>Game servers</h2>
    <div class="small muted">Published in the auto-config: players get them as favorite servers.</div>
    {#each servers as srv, i}
      <div class="row">
        <input placeholder="Name" bind:value={srv.name} style="flex:2" />
        <input placeholder="Host" bind:value={srv.host} style="flex:2" />
        <input placeholder="Port" type="number" bind:value={srv.port} style="flex:1" />
        <input placeholder="Password" bind:value={srv.password} style="flex:1" />
        <input placeholder="Event / modset" bind:value={srv.modset} style="flex:2" />
        <button class="btn ghost danger small" onclick={() => servers.splice(i, 1)}>✕</button>
      </div>
    {/each}
    <div><button class="btn small" onclick={() => servers.push({ name: "", host: "", port: 2302 })}>Add server</button></div>
  </div>

  <div class="row end"><button class="btn primary" disabled={busy} onclick={save}>Save settings</button></div>
</div>
