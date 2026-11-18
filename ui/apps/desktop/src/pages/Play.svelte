<script lang="ts">
  import { onMount } from "svelte";
  import { toast, navigate } from "@axs/kit";
  import { call, selectedAddons, type Profile } from "../api";
  import { app, refresh } from "../store.svelte";

  let profile = $state<Profile | null>(null);
  let server = $state("");
  let preview = $state<any>(null);
  let starting = $state(false);

  async function load() {
    profile = await call<Profile>("profile_get", { name: app.state!.active_profile });
    server = profile.server ?? "";
    await updatePreview();
  }
  onMount(load);

  async function updatePreview() {
    if (!profile) return;
    preview = await call("launch_preview", { profile, server: server || null });
  }

  async function switchProfile(name: string) {
    await call("profile_activate", { name });
    await refresh();
    await load();
  }

  async function chooseServer(name: string) {
    server = name;
    if (profile) {
      profile.server = name || null;
      // Joining with a modset: select that group only.
      const fav = app.state?.config.favorite_servers.find((s: any) => s.name === name);
      if (fav?.modset) {
        const target = profile.groups.find((g) => g.type === "group" && g.name.toLowerCase() === fav.modset.toLowerCase());
        if (target) {
          const setAll = (n: any, v: boolean) => (n.type === "group" ? n.children.forEach((c: any) => setAll(c, v)) : (n.selected = v && !n.optional));
          profile.groups.forEach((g) => setAll(g, false));
          setAll(target, true);
        }
      }
      await call("profile_save", { profile, original: profile.name });
    }
    await updatePreview();
  }

  const outdated = $derived(app.state?.repos.filter((r) => r.state.status === "updated") ?? []);
  const selected = $derived(profile ? selectedAddons(profile.groups) : []);

  async function start() {
    if (!profile) return;
    if (preview?.check?.missing_addons?.length && !confirm(`Missing addons: ${preview.check.missing_addons.join(", ")}\nStart anyway?`)) return;
    if (Object.keys(app.progress).length && !confirm("Files are still downloading. Start anyway?")) return;
    if (outdated.length && !confirm(`Updates available for ${outdated.map((r) => r.name).join(", ")}. Start anyway?`)) return;
    starting = true;
    try {
      const pid = await call<number>("launch_game", { profile, server: server || null });
      toast(`Arma 3 started (pid ${pid})`);
    } catch (e: any) {
      toast(String(e), "err", 8000);
    } finally {
      starting = false;
      updatePreview();
    }
  }
</script>

{#if profile && app.state}
  <div class="hero card stack">
    <div class="row">
      <div>
        <h1>Play Arma 3</h1>
        <div class="muted">{selected.length} addon(s) selected</div>
      </div>
      <div class="spacer"></div>
      <button class="btn primary big" disabled={starting || !!preview?.error} onclick={start}>{starting ? "Starting…" : "▶ Start game"}</button>
    </div>
    <div class="grid2">
      <label class="field">Profile
        <select value={profile.name} onchange={(e) => switchProfile((e.target as HTMLSelectElement).value)}>
          {#each app.state.profiles as p}<option value={p}>{p}</option>{/each}
        </select>
      </label>
      <label class="field">Join server
        <select value={server} onchange={(e) => chooseServer((e.target as HTMLSelectElement).value)}>
          <option value="">— none —</option>
          {#each app.state.config.favorite_servers as s}<option value={s.name}>{s.name}{s.modset ? ` · ${s.modset}` : ""}</option>{/each}
        </select>
      </label>
    </div>
  </div>

  <div class="stack" style="margin-top:12px">
    {#if preview?.error}<div class="alert err">{preview.error} — set the game executable in <a href="#/settings">Settings</a>.</div>{/if}
    {#if preview?.running}<div class="alert warn">Arma 3 is already running.</div>{/if}
    {#if preview?.check?.missing_addons?.length}
      <div class="alert warn">Missing addons: {preview.check.missing_addons.join(", ")} — <a href="#/repos">synchronize a repository</a> or add search folders in <a href="#/settings">Settings</a>.</div>
    {/if}
    {#if preview?.check?.duplicate_selection?.length}<div class="alert warn">Selected more than once: {preview.check.duplicate_selection.join(", ")}</div>{/if}
    {#each outdated as r}
      <div class="alert warn row">{r.name} has updates (revision {r.state.remote_revision}).<div class="spacer"></div><button class="btn small" onclick={() => navigate(`/repos/${encodeURIComponent(r.name)}`)}>Open</button></div>
    {/each}

    <div class="card">
      <div class="row"><h2 style="margin:0">Launch parameters</h2><div class="spacer"></div><a href="#/modsets" class="small">Edit modsets</a> · <a href="#/settings" class="small">Options</a></div>
      <div class="log" style="margin-top:8px">{preview ? [preview.command?.program ?? "<executable>", ...(preview.command?.args ?? preview.args)].join("\n") : "…"}</div>
    </div>
  </div>
{:else}
  <div class="empty">Loading…</div>
{/if}

<style>
  .hero { background: linear-gradient(135deg, var(--accent-soft), var(--panel) 60%); }
</style>
