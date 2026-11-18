<script lang="ts">
  import { onMount } from "svelte";
  import { toast } from "@axs/kit";
  import { call, pickFile, pickFolder, type Profile } from "../api";
  import { app, refresh } from "../store.svelte";

  let profile = $state<Profile | null>(null);
  let originalName = $state("");
  // svelte-ignore state_referenced_locally
  let cfg: any = $state(structuredClone($state.snapshot(app.state!.config)));
  let info: any = $state({ game_profiles: ["Default"], mallocs: [], cpus: 8 });

  async function load(name = app.state!.active_profile) {
    profile = await call<Profile>("profile_get", { name });
    originalName = profile.name;
    info = await call("game_info", { profile });
  }
  onMount(() => load());

  async function saveProfile() {
    if (!profile) return;
    try {
      await call("profile_save", { profile, original: originalName });
      if (originalName !== profile.name && app.state!.active_profile === originalName) await call("profile_activate", { name: profile.name });
      originalName = profile.name;
      await refresh();
      toast("Profile saved");
    } catch (e: any) {
      toast(String(e), "err");
    }
  }
  async function saveApp() {
    try {
      await call("save_config", { config: cfg });
      await refresh();
      toast("Settings saved");
    } catch (e: any) {
      toast(String(e), "err");
    }
  }
  async function newProfile(copy: boolean) {
    const name = prompt("Profile name");
    if (!name || !profile) return;
    const p = copy ? { ...$state.snapshot(profile), name } : { ...$state.snapshot(profile), name, groups: [], addon_priority: [], additional_parameters: "" };
    await call("profile_save", { profile: p, original: null });
    await call("profile_activate", { name });
    await refresh();
    await load(name);
  }
  async function deleteProfile() {
    if (!profile || profile.name === "Default" || !confirm(`Delete profile ${profile.name}?`)) return;
    await call("profile_remove", { name: profile.name });
    await call("profile_activate", { name: "Default" });
    await refresh();
    await load("Default");
  }
  async function exe() {
    const f = await pickFile("Arma 3 executable", ["exe", "sh", "bat", "*"]);
    if (f && profile) {
      profile.executable = f;
      info = await call("game_info", { profile });
    }
  }
  function toggleBattleye() {
    if (!profile?.executable) return;
    const e = profile.executable;
    profile.executable = /arma3battleye\.exe$/i.test(e) ? e.replace(/arma3battleye\.exe$/i, "arma3_x64.exe") : e.replace(/arma3(_x64)?\.exe$/i, "arma3battleye.exe");
  }
  const battleye = $derived(/battleye/i.test(profile?.executable ?? ""));
  const cpuChoices = $derived(Array.from({ length: info.cpus }, (_: unknown, i: number) => i + 1));
  const flags: [string, string][] = [
    ["show_script_errors", "Show script errors"],
    ["no_pause", "No pause"],
    ["window_mode", "Window mode"],
    ["file_patching", "File patching"],
    ["check_signatures", "Check signatures"],
    ["enable_ht", "Enable hyper-threading"],
    ["huge_pages", "Huge pages"],
    ["no_splash", "No splash screen"],
    ["default_world_empty", "Empty default world"],
    ["no_logs", "No logs"],
    ["auto_restart", "Auto-restart the game"],
  ];
</script>

<h1>Settings</h1>
{#if profile}
  <div class="stack" style="margin-top:12px">
    <div class="card stack">
      <div class="row">
        <h2 style="margin:0">Launcher profile</h2>
        <select style="width:auto" value={profile.name} onchange={async (e) => { const n = (e.target as HTMLSelectElement).value; await call("profile_activate", { name: n }); await refresh(); await load(n); }}>
          {#each app.state!.profiles as p}<option value={p}>{p}</option>{/each}
        </select>
        <div class="spacer"></div>
        <button class="btn small" onclick={() => newProfile(false)}>New</button>
        <button class="btn small" onclick={() => newProfile(true)}>Duplicate</button>
        <button class="btn small danger" disabled={profile.name === "Default"} onclick={deleteProfile}>Delete</button>
      </div>
      <label class="field">Profile name <input bind:value={profile.name} disabled={profile.name === "Default" && originalName === "Default"} /></label>
      <label class="field">Arma 3 executable
        <div class="row"><input bind:value={profile.executable} style="flex:1" placeholder="…/Arma 3/arma3_x64.exe" /><button class="btn" onclick={exe}>Browse…</button>
          <button class="btn" onclick={toggleBattleye} title="Switch between arma3_x64.exe and arma3battleye.exe">BattlEye {battleye ? "on" : "off"}</button></div>
      </label>
      <div class="row">
        <label class="field" style="flex:1">Start the game
          <select bind:value={profile.launch_mode}><option value="direct">directly (executable)</option><option value="steam">through Steam</option></select>
        </label>
        {#if app.state!.game.linux}<label class="check"><input type="checkbox" bind:checked={profile.proton} /> Proton (Linux)</label>{/if}
        <label class="check"><input type="checkbox" bind:checked={profile.use_workshop} /> Use Steam Workshop mods</label>
      </div>
      <div class="field">Addon search folders
        {#each profile.addon_search_dirs as d, i}
          <div class="row"><code style="flex:1">{d}</code><button class="btn ghost small danger" onclick={() => profile!.addon_search_dirs.splice(i, 1)}>✕</button></div>
        {/each}
        <div><button class="btn small" onclick={async () => { const d = await pickFolder(); if (d) profile!.addon_search_dirs.push(d); }}>Add folder</button></div>
        <span class="small muted">Repository destinations and the game folder are always searched.</span>
      </div>
      <h3>Options</h3>
      <div class="grid2">
        <label class="field">Game profile (-name)
          <select bind:value={profile.options.game_profile}>{#each info.game_profiles as g}<option value={g === "Default" ? null : g}>{g}</option>{/each}</select>
        </label>
        <label class="field">Memory allocator (-malloc)
          <select bind:value={profile.options.malloc}><option value={null}>default</option>{#each info.mallocs as m}<option value={m}>{m}</option>{/each}</select>
        </label>
        <label class="field">Max memory MB (-maxMem)
          <select bind:value={profile.options.max_mem}><option value={null}>default</option>{#each [1024, 2047, 3071, 4096, 8192, 16384] as m}<option value={m}>{m}</option>{/each}</select>
        </label>
        <label class="field">CPU count (-cpuCount)
          <select bind:value={profile.options.cpu_count}><option value={null}>default</option>{#each cpuChoices as c}<option value={c}>{c}</option>{/each}</select>
        </label>
        <label class="field">Extra threads (-exThreads)
          <select bind:value={profile.options.ex_threads}><option value={null}>default</option>{#each [0, 1, 3, 5, 7] as c}<option value={c}>{c}</option>{/each}</select>
        </label>
      </div>
      <div class="flags">
        {#each flags as [k, l]}<label class="check"><input type="checkbox" bind:checked={profile.options[k]} /> {l}</label>{/each}
      </div>
      <label class="field">Additional parameters <input bind:value={profile.additional_parameters} placeholder="-skipIntro -noLauncher" /></label>
      <div class="row end"><button class="btn primary" onclick={saveProfile}>Save profile</button></div>
    </div>

    <div class="card stack">
      <h2>External applications</h2>
      <div class="small muted">Started before the game (TeamSpeak, overlays…), unless already running.</div>
      {#each cfg.external_apps as a, i}
        <div class="row">
          <input type="checkbox" bind:checked={a.enabled} title="Enabled" />
          <input bind:value={a.name} placeholder="Name" style="flex:1" />
          <input bind:value={a.path} placeholder="Path" style="flex:2" />
          <button class="btn small" onclick={async () => { const f = await pickFile(); if (f) a.path = f; }}>…</button>
          <input bind:value={a.parameters} placeholder="Parameters" style="flex:1" />
          <button class="btn ghost small danger" onclick={() => cfg.external_apps.splice(i, 1)}>✕</button>
        </div>
      {/each}
      <div><button class="btn small" onclick={() => cfg.external_apps.push({ name: "", path: "", parameters: "", enabled: true })}>Add application</button></div>
    </div>

    <div class="card stack">
      <h2>Application</h2>
      <div class="grid2">
        <label class="field">Default download folder
          <div class="row"><input bind:value={cfg.default_destination} style="flex:1" /><button class="btn" onclick={async () => (cfg.default_destination = (await pickFolder()) ?? cfg.default_destination)}>…</button></div>
        </label>
        <label class="field">Check repositories every (minutes) <span class="hint">While the app is open; 0 = only at start.</span><input type="number" min="0" bind:value={cfg.check_interval_minutes} /></label>
        <label class="field">Global speed limit (MB/s) <span class="hint">0 = unlimited</span>
          <input type="number" min="0" step="0.5" value={cfg.max_speed / 1e6} oninput={(e) => (cfg.max_speed = Math.round(Number((e.target as HTMLInputElement).value) * 1e6))} />
        </label>
        <label class="field">Proxy <span class="hint">http://, https:// or socks5:// URL</span><input bind:value={cfg.proxy} /></label>
        <label class="field">When the game starts
          <select bind:value={cfg.at_launch}><option value="nothing">do nothing</option><option value="minimize">minimize ArmaXSync</option><option value="close">close ArmaXSync</option></select>
        </label>
        <label class="field">Parallel download of big files (MB) <span class="hint">0 = off; ArmaXSync servers only.</span>
          <input type="number" min="0" value={cfg.segment_threshold / 1e6} oninput={(e) => (cfg.segment_threshold = Math.round(Number((e.target as HTMLInputElement).value) * 1e6))} />
        </label>
      </div>
      <div class="small muted">Data folder: {app.state!.data_dir}{app.state!.portable ? " (portable)" : ""}</div>
      <div class="row end"><button class="btn primary" onclick={saveApp}>Save</button></div>
    </div>
  </div>
{:else}
  <div class="empty">Loading…</div>
{/if}

<style>
  .flags { display: grid; grid-template-columns: repeat(auto-fill, minmax(210px, 1fr)); gap: 6px; }
</style>
