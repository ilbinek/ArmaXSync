<script lang="ts">
  import { onMount } from "svelte";
  import { Progress, bytes, speed, date, toast, navigate, duration } from "@axs/kit";
  import { call, pickFolder, type RepoConfig } from "../api";
  import { app, refresh, startSync } from "../store.svelte";

  let { name }: { name: string } = $props();
  let tab = $state("sync");
  let info = $state<any>(null);
  let infoError = $state("");
  let plan = $state<any>(null);
  let planning = $state(false);
  let event = $state("");
  let edit = $state<RepoConfig | null>(null);
  let adminBusy = $state(false);
  let adminLog = $state("");

  const repo = $derived(app.state?.repos.find((r) => r.name === name));
  const progress = $derived(app.progress[name]);

  onMount(async () => {
    edit = structuredClone($state.snapshot(repo)) as RepoConfig;
    try {
      info = await call("repo_info", { name });
    } catch (e: any) {
      infoError = String(e);
    }
  });

  async function preview() {
    planning = true;
    try {
      plan = await call("sync_plan", { name, params: { event: event || null } });
    } catch (e: any) {
      toast(String(e), "err");
    } finally {
      planning = false;
    }
  }

  async function sync() {
    plan = null;
    try {
      await startSync(name, { event: event || null });
    } catch (e: any) {
      toast(String(e), "err");
    }
  }

  async function save() {
    if (!edit) return;
    try {
      await call("repo_save", { repo: edit, original: name });
      await refresh();
      toast("Saved");
      if (edit.name !== name) navigate(`/repos/${encodeURIComponent(edit.name)}`);
    } catch (e: any) {
      toast(String(e), "err");
    }
  }

  async function remove() {
    if (!confirm(`Remove ${name}? Downloaded files are kept.`)) return;
    await call("repo_remove", { name });
    await refresh();
    navigate("/repos");
  }

  async function addModset(ev?: string) {
    try {
      await call("apply_modset", { profile: app.state!.active_profile, repo: name, event: ev ?? null });
      toast(`Modset ${ev ?? name} added to profile ${app.state!.active_profile}`);
    } catch (e: any) {
      toast(String(e), "err");
    }
  }

  async function chooseDest() {
    const d = await pickFolder("Download destination");
    if (d && edit) edit.destination = d;
  }

  function ensureAdmin() {
    if (edit && !edit.admin) edit.admin = { path: null, public_url: null, compress: false, compressed_only: false, connections: 3, partial_transfer: true, embed_credentials: true, exclude: [], hidden: [], servers: [], push_server: null, push_repo: null, push_token: null };
  }

  async function build() {
    adminBusy = true;
    adminLog = "Building…";
    try {
      await save();
      const r: any = await call("build_repo", { name });
      adminLog = `Revision ${r.revision}: ${r.files} files, ${bytes(r.total_size)} (${r.hashed_files} hashed) in ${(r.duration_ms / 1000).toFixed(1)} s`;
    } catch (e: any) {
      adminLog = String(e);
    } finally {
      adminBusy = false;
    }
  }

  async function push() {
    adminBusy = true;
    adminLog = "Pushing…";
    try {
      await save();
      const r: any = await call("push_repo", { name, deleteMissing: true });
      adminLog = `Pushed: ${bytes(r.bytes_sent)} sent for ${bytes(r.bytes_total)} (${r.delta_files} delta, ${r.full_files} full, ${r.deleted} deleted) → revision ${r.revision ?? "-"}`;
    } catch (e: any) {
      adminLog = String(e);
    } finally {
      adminBusy = false;
    }
  }
</script>

{#if !repo}
  <div class="empty">Repository not found. <a href="#/repos">Back</a></div>
{:else}
  <div class="small"><a href="#/repos">Repositories</a> /</div>
  <div class="row" style="margin-bottom:10px">
    <div>
      <h1>{repo.name}</h1>
      <div class="muted small">{repo.url} · revision {repo.state.revision ?? "-"}{info ? ` · remote ${info.revision} (${info.source}) · ${info.files} files · ${bytes(info.size)}` : ""}</div>
    </div>
  </div>
  <div class="tabs">
    {#each [["sync", "Synchronize"], ["events", "Events"], ["changelog", "Changelog"], ["settings", "Settings"], ["admin", "Administration"]] as [id, l]}
      <button class:active={tab === id} onclick={() => (tab = id)}>{l}</button>
    {/each}
  </div>
  {#if infoError}<div class="alert err" style="margin-bottom:10px">{infoError}</div>{/if}

  {#if tab === "sync"}
    <div class="stack">
      <div class="card stack">
        <div class="row">
          <label class="field" style="flex:1">What to download
            <select bind:value={event}>
              <option value="">Whole repository</option>
              {#each info?.events ?? [] as e}<option value={e.name}>Event: {e.name}</option>{/each}
            </select>
          </label>
          <label class="field" style="flex:2">Destination
            <input value={repo.destination ?? ""} disabled />
          </label>
        </div>
        {#if progress}
          <Progress value={progress.bytes_done} max={progress.bytes_total} label={`${progress.files_done}/${progress.files_total} files · ${bytes(progress.bytes_done)} / ${bytes(progress.bytes_total)} · ${speed(progress.speed)}${progress.speed > 0 ? " · " + duration((progress.bytes_total - progress.bytes_done) / progress.speed) + " left" : ""}`} />
          <div class="row"><span class="small muted">{progress.current}</span><div class="spacer"></div><button class="btn" onclick={() => call("sync_cancel", { name })}>Cancel</button></div>
        {:else}
          <div class="row end">
            <button class="btn" disabled={planning || !repo.destination} onclick={preview}>{planning ? "Checking files…" : "Preview changes"}</button>
            <button class="btn primary" disabled={!repo.destination} onclick={sync}>Synchronize</button>
          </div>
        {/if}
      </div>
      {#if plan}
        <div class="card stack">
          <h2>{plan.plan.fetch_count} file(s) to download ({bytes(plan.plan.fetch_bytes)}), {plan.plan.delete_count} to delete</h2>
          <div class="log">{plan.plan.ops.filter((o: any) => o.op !== "mkdir").map((o: any) => (o.op === "fetch" ? `${o.local_size != null ? "update" : "new   "} ${o.path} (${bytes(o.size)})` : `delete ${o.path}${o.dir ? "/" : ""}`)).join("\n") || "Everything is up to date."}</div>
        </div>
      {/if}
      <div class="row"><button class="btn" onclick={() => addModset()}>Add repository modset to profile</button></div>
    </div>
  {:else if tab === "events"}
    <div class="stack">
      {#each info?.events ?? [] as e}
        <div class="card">
          <div class="row"><b>{e.name}</b><span class="muted">{e.description ?? ""}</span><div class="spacer"></div><button class="btn small" onclick={() => addModset(e.name)}>Add to profile</button><button class="btn small primary" onclick={() => { event = e.name; tab = "sync"; }}>Download</button></div>
          <div class="small" style="margin-top:6px">{e.addons.map((a: any) => a.name + (a.optional ? " (optional)" : "")).join(", ")}</div>
        </div>
      {:else}<div class="card empty">No events in this repository.</div>{/each}
    </div>
  {:else if tab === "changelog"}
    <div class="card flush">
      <table>
        <thead><tr><th>Revision</th><th>Date</th><th>Changes</th></tr></thead>
        <tbody>
          {#each [...(info?.changelogs ?? [])].reverse() as c}
            <tr>
              <td>{c.revision}</td><td class="small">{date(c.build_date)}</td>
              <td class="small">
                {#if c.new_addons.length}<div><b>new:</b> {c.new_addons.join(", ")}</div>{/if}
                {#if c.updated_addons.length}<div><b>updated:</b> {c.updated_addons.join(", ")}</div>{/if}
                {#if c.deleted_addons.length}<div><b>removed:</b> {c.deleted_addons.join(", ")}</div>{/if}
                {#if !c.new_addons.length && !c.updated_addons.length && !c.deleted_addons.length}<span class="muted">{c.content_updated ? "content updated" : "no content change"}</span>{/if}
              </td>
            </tr>
          {:else}<tr><td colspan="3" class="empty">No changelog.</td></tr>{/each}
        </tbody>
      </table>
    </div>
  {:else if tab === "settings" && edit}
    <div class="stack">
      <div class="card stack">
        <div class="grid2">
          <label class="field">Name <input bind:value={edit.name} /></label>
          <label class="field">URL <input bind:value={edit.url} /></label>
          <label class="field">Login <input bind:value={edit.login} placeholder="anonymous" /></label>
          <label class="field">Password <input type="password" bind:value={edit.password} /></label>
        </div>
        <label class="field">Destination folder
          <div class="row"><input bind:value={edit.destination} style="flex:1" /><button class="btn" onclick={chooseDest}>Browse…</button></div>
        </label>
        <div class="row">
          <label class="check"><input type="checkbox" bind:checked={edit.notify} /> Notify about updates</label>
          <label class="check"><input type="checkbox" bind:checked={edit.auto} /> Download updates automatically</label>
          <label class="check"><input type="checkbox" bind:checked={edit.exact_match} /> Exact match (also remove extra files in the destination root)</label>
          <label class="check"><input type="checkbox" bind:checked={edit.auto_discover} /> Reuse addons found in other folders</label>
        </div>
        <div class="grid2">
          <label class="field">Max connections <span class="hint">0 = repository default</span><input type="number" min="0" max="10" bind:value={edit.connections} /></label>
          <label class="field">Speed limit (MB/s) <span class="hint">0 = unlimited</span>
            <input type="number" min="0" step="0.5" value={edit.max_speed / 1e6} oninput={(e) => (edit!.max_speed = Math.round(Number((e.target as HTMLInputElement).value) * 1e6))} />
          </label>
          <label class="field">Connect timeout (s) <input type="number" min="0" bind:value={edit.connect_timeout_secs} /></label>
          <label class="field">Read timeout (s) <input type="number" min="0" bind:value={edit.read_timeout_secs} /></label>
        </div>
        <label class="field">Keep my extra files in these folders <span class="hint">One per line, relative to the destination.</span>
          <textarea value={edit.keep_extras.join("\n")} oninput={(e) => (edit!.keep_extras = (e.target as HTMLTextAreaElement).value.split("\n").map((s) => s.trim()).filter(Boolean))}></textarea>
        </label>
        <label class="check"><input type="checkbox" bind:checked={edit.accept_invalid_certs} /> Accept invalid HTTPS certificates (not recommended)</label>
        <div class="row"><button class="btn danger" onclick={remove}>Remove repository</button><div class="spacer"></div><button class="btn primary" onclick={save}>Save</button></div>
      </div>
    </div>
  {:else if tab === "admin" && edit}
    <div class="stack">
      <div class="card stack">
        <h2>Build locally</h2>
        <div class="small muted">For communities hosting the repository with any web server: build the repository files from a local folder, then upload it yourself.</div>
        {#if !edit.admin}<div><button class="btn" onclick={ensureAdmin}>Enable administration</button></div>{:else}
          <label class="field">Repository folder
            <div class="row"><input bind:value={edit.admin.path} style="flex:1" /><button class="btn" onclick={async () => { const d = await pickFolder(); if (d && edit?.admin) edit.admin.path = d; }}>Browse…</button></div>
          </label>
          <div class="grid2">
            <label class="field">Public URL <span class="hint">Defaults to the repository URL.</span><input bind:value={edit.admin.public_url} /></label>
            <label class="field">Connections per client <input type="number" min="1" max="10" bind:value={edit.admin.connections} /></label>
          </div>
          <div class="row">
            <label class="check"><input type="checkbox" bind:checked={edit.admin.compress} /> Compressed PBOs</label>
            <label class="check"><input type="checkbox" bind:checked={edit.admin.partial_transfer} /> Partial transfers</label>
            <label class="check"><input type="checkbox" bind:checked={edit.admin.embed_credentials} /> Embed login in auto-config</label>
          </div>
          <div class="row end"><button class="btn primary" disabled={adminBusy || !edit.admin.path} onclick={build}>Build repository</button></div>
          <hr />
          <h2>Push to an ArmaXSync server</h2>
          <div class="grid2">
            <label class="field">Server URL <input bind:value={edit.admin.push_server} placeholder="https://mods.example.com" /></label>
            <label class="field">Server repository (slug) <input bind:value={edit.admin.push_repo} /></label>
          </div>
          <label class="field">API token <span class="hint">Create one under API tokens in the server's admin page.</span><input type="password" bind:value={edit.admin.push_token} /></label>
          <div class="row end"><button class="btn primary" disabled={adminBusy || !edit.admin.path || !edit.admin.push_server} onclick={push}>Push changes</button></div>
        {/if}
        {#if adminLog}<div class="log">{adminLog}</div>{/if}
      </div>
    </div>
  {/if}
{/if}
