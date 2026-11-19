<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Section, Field, PathInput, Switch, SegmentedControl, SaveBar, Button, Badge, Alert, ThemeToggle, Skeleton, router, setQuery, useUnsavedGuard, snapshot, toast, toastError, errorText, confirmDialog, promptDialog } from "@axs/kit";
  import { Gamepad2, SlidersHorizontal, AppWindow, Download, ArrowUpCircle, Palette, Info, Plus, X, FolderPlus, Wand2, ExternalLink, FolderOpen, Trash2, Pencil, RefreshCw } from "@lucide/svelte";
  import { call, pickFile, pickFolder, openPath, openUrl, type Profile, type AppConfig, type GameDetails, type LauncherOptions } from "../api";
  import { app, refresh, loadProfile, switchProfile, checkForUpdate } from "../store.svelte";

  const sections = [
    { id: "game", label: "Game", icon: Gamepad2 },
    { id: "launch", label: "Launch options", icon: SlidersHorizontal },
    { id: "apps", label: "External apps", icon: AppWindow },
    { id: "downloads", label: "Downloads", icon: Download },
    { id: "updates", label: "Updates", icon: ArrowUpCircle },
    { id: "appearance", label: "Appearance", icon: Palette },
    { id: "about", label: "About", icon: Info },
  ];
  const section = $derived(sections.some((s) => s.id === router.query.get("section")) ? router.query.get("section")! : "game");

  let profile = $state<Profile | null>(null);
  let cfg = $state<AppConfig | null>(null);
  let savedProfile = $state("");
  let savedCfg = $state("");
  let originalName = $state("");
  let info = $state<GameDetails>({ game_profiles: [], mallocs: [], cpus: 8, running: false });
  let saving = $state(false);

  async function load() {
    await loadProfile();
    profile = structuredClone($state.snapshot(app.profile)) as Profile;
    originalName = profile.name;
    savedProfile = snapshot(profile);
    cfg = structuredClone($state.snapshot(app.state!.config)) as AppConfig;
    savedCfg = snapshot(cfg);
    info = await call<GameDetails>("game_info", { profile }).catch(() => info);
  }
  onMount(load);

  const profileDirty = $derived(!!profile && snapshot(profile) !== savedProfile);
  const cfgDirty = $derived(!!cfg && snapshot(cfg) !== savedCfg);
  const dirty = $derived(profileDirty || cfgDirty);
  useUnsavedGuard(() => dirty);

  async function save() {
    if (!profile || !cfg) return;
    saving = true;
    try {
      if (profileDirty) {
        await call("profile_save", { profile: $state.snapshot(profile), original: originalName });
        if (originalName !== profile.name && app.state!.active_profile === originalName) await call("profile_activate", { name: profile.name });
        originalName = profile.name;
        savedProfile = snapshot(profile);
      }
      if (cfgDirty) {
        // Keep fields other pages may have changed meanwhile.
        const fresh = (await call<any>("app_state")).config;
        const merged = { ...fresh, ...$state.snapshot(cfg), favorite_servers: fresh.favorite_servers, profile: fresh.profile };
        await call("save_config", { config: merged });
        savedCfg = snapshot(cfg);
      }
      await refresh();
      await loadProfile();
      toast("Settings saved");
    } catch (e) {
      toastError(e);
    } finally {
      saving = false;
    }
  }

  function discard() {
    profile = JSON.parse(savedProfile);
    cfg = JSON.parse(savedCfg);
  }

  async function changeProfile(name: string) {
    if (dirty && !(await confirmDialog({ title: "Discard changes?", message: "Switching profiles discards your unsaved changes.", confirmLabel: "Discard", danger: true }))) return;
    await switchProfile(name);
    await load();
  }

  async function renameProfile() {
    if (!profile) return;
    const name = await promptDialog({ title: "Rename profile", label: "Profile name", value: profile.name, validate: (v) => (v !== originalName && app.state!.profiles.includes(v) ? "A profile with this name exists" : null) });
    if (name) profile.name = name;
  }

  async function deleteProfile() {
    if (!profile || app.state!.profiles.length < 2) return;
    if (!(await confirmDialog({ title: `Delete profile ${originalName}?`, message: "Its modset selection and options are deleted. Downloaded addons are not affected.", confirmLabel: "Delete", danger: true }))) return;
    try {
      await call("profile_remove", { name: originalName });
      savedProfile = snapshot(profile);
      await switchProfile(app.state!.profiles.find((p) => p !== originalName)!);
      await load();
    } catch (e) {
      toastError(e);
    }
  }

  async function pickExe() {
    const f = await pickFile("Arma 3 executable", ["exe", "sh", "bat"]);
    if (f && profile) {
      profile.executable = f;
      info = await call<GameDetails>("game_info", { profile: $state.snapshot(profile) }).catch(() => info);
    }
    return null;
  }
  const detectedExe = $derived.by(() => {
    const d = app.state?.game.arma3_dir;
    if (!d) return null;
    return `${d}${app.state!.game.linux ? "/" : "\\"}arma3_x64.exe`;
  });
  const battleye = $derived(/battleye/i.test(profile?.executable ?? ""));
  function setBattleye(on: boolean) {
    if (!profile?.executable) return;
    const e = profile.executable;
    profile.executable = on ? e.replace(/arma3(_x64)?\.exe$/i, "arma3battleye.exe") : e.replace(/arma3battleye\.exe$/i, "arma3_x64.exe");
  }

  async function addSearchDir() {
    const d = await pickFolder("Addon folder");
    if (d && profile && !profile.addon_search_dirs.includes(d)) profile.addon_search_dirs.push(d);
  }

  const flags: { k: keyof LauncherOptions; label: string; hint: string }[] = [
    { k: "no_splash", label: "Skip intro screens", hint: "-noSplash" },
    { k: "default_world_empty", label: "Empty main menu world", hint: "-world=empty · faster start" },
    { k: "no_pause", label: "Keep running in background", hint: "-noPause" },
    { k: "window_mode", label: "Windowed mode", hint: "-window" },
    { k: "show_script_errors", label: "Show script errors", hint: "-showScriptErrors · for mission makers" },
    { k: "file_patching", label: "File patching", hint: "-filePatching · load unpacked files" },
    { k: "check_signatures", label: "Check signatures", hint: "-checkSignatures" },
    { k: "enable_ht", label: "Hyper-threading", hint: "-enableHT" },
    { k: "huge_pages", label: "Huge pages", hint: "-hugePages" },
    { k: "no_logs", label: "No logs", hint: "-noLogs · makes troubleshooting harder" },
    { k: "auto_restart", label: "Restart after crash", hint: "Relaunch the game if it exits unexpectedly" },
  ];
  const cpuChoices = $derived(Array.from({ length: info.cpus }, (_, i) => i + 1));

  let speedMb = $state(0);
  let segMb = $state(0);
  $effect.pre(() => {
    if (cfg) {
      speedMb = cfg.max_speed / 1e6;
      segMb = cfg.segment_threshold / 1e6;
    }
  });
</script>

<div class="page">
  <PageHeader title="Settings" />
  {#if !profile || !cfg}
    <Skeleton lines={8} />
  {:else}
    <div class="layout">
      <nav class="subnav" aria-label="Settings sections">
        {#each sections as s}
          <button class:active={section === s.id} onclick={() => setQuery("section", s.id === "game" ? null : s.id)} aria-current={section === s.id ? "page" : undefined}>
            <s.icon size={16} />{s.label}
          </button>
        {/each}
      </nav>

      <div class="stack loose content">
        {#if section === "game" || section === "launch"}
          <div class="profbar card raised">
            <span class="eyebrow">Profile</span>
            <select value={app.state!.active_profile} onchange={(e) => changeProfile((e.target as HTMLSelectElement).value)} style="width:auto;min-width:180px">
              {#each app.state!.profiles as p}<option value={p}>{p}</option>{/each}
            </select>
            <span class="xs muted grow">These settings belong to this profile.</span>
            <Button size="sm" variant="ghost" icon={Pencil} onclick={renameProfile}>Rename</Button>
            <Button size="sm" variant="ghost" icon={Trash2} disabled={app.state!.profiles.length < 2} onclick={deleteProfile}>Delete</Button>
          </div>
        {/if}

        {#if section === "game"}
          <Section title="Game executable" icon={Gamepad2}>
            <Field label="Executable" hint={info.running ? "Arma 3 is running." : undefined}>
              {#snippet children(id)}<PathInput {id} bind:value={profile!.executable} placeholder="arma3_x64.exe" browse={pickExe} />{/snippet}
            </Field>
            {#if detectedExe && profile.executable !== detectedExe && !battleye}
              <div class="row"><span class="small muted">Detected: <code>{detectedExe}</code></span><Button size="sm" icon={Wand2} onclick={() => (profile!.executable = detectedExe)}>Use it</Button></div>
            {/if}
            <Switch checked={battleye} disabled={!profile.executable} onchange={setBattleye} label="BattlEye" hint="Start through arma3battleye.exe; required by most public servers." />
            <Field label="Start the game">
              {#snippet children()}
                <SegmentedControl bind:value={profile!.launch_mode} options={[{ value: "direct", label: "Directly" }, { value: "steam", label: "Through Steam" }]} />
              {/snippet}
            </Field>
            {#if app.state!.game.linux}<Switch bind:checked={profile.proton} label="Run with Proton" hint="Linux: start the Windows game through Steam's Proton." />{/if}
          </Section>

          <Section title="Addon folders" icon={FolderOpen} description="Where ArmaXSync looks for @addons. Repository download folders are always included.">
            <Switch bind:checked={profile.use_workshop} label="Include Steam Workshop items" />
            {#if profile.addon_search_dirs.length}
              <ul class="list">
                {#each profile.addon_search_dirs as d, i}
                  <li><FolderOpen size={15} class="subtle" /><span class="mono small grow ellipsis" title={d}>{d}</span><button class="btn ghost icon sm" aria-label={`Remove ${d}`} onclick={() => profile!.addon_search_dirs.splice(i, 1)}><X size={14} /></button></li>
                {/each}
              </ul>
            {/if}
            <div><Button size="sm" icon={FolderPlus} onclick={addSearchDir}>Add folder</Button></div>
          </Section>
        {:else if section === "launch"}
          <Section title="Game options" icon={SlidersHorizontal}>
            <div class="form-grid">
              <Field label="Player profile" hint="-name">
                {#snippet children(id)}
                  <select {id} bind:value={profile!.options.game_profile}><option value={null}>Default</option>{#each info.game_profiles as g}<option value={g}>{g}</option>{/each}</select>
                {/snippet}
              </Field>
              <Field label="Memory allocator" hint="-malloc">
                {#snippet children(id)}
                  <select {id} bind:value={profile!.options.malloc}><option value={null}>Default</option>{#each info.mallocs as m}<option value={m}>{m}</option>{/each}</select>
                {/snippet}
              </Field>
              <Field label="Max memory" hint="-maxMem · leave on automatic unless told otherwise">
                {#snippet children(id)}
                  <select {id} bind:value={profile!.options.max_mem}><option value={null}>Automatic</option>{#each [2047, 4096, 8192, 12288, 16384, 24576, 32768] as m}<option value={m}>{m >= 1024 ? `${Math.round(m / 1024)} GB` : `${m} MB`}</option>{/each}</select>
                {/snippet}
              </Field>
              <Field label="CPU cores" hint="-cpuCount">
                {#snippet children(id)}
                  <select {id} bind:value={profile!.options.cpu_count}><option value={null}>Automatic</option>{#each cpuChoices as c}<option value={c}>{c}</option>{/each}</select>
                {/snippet}
              </Field>
              <Field label="Extra threads" hint="-exThreads">
                {#snippet children(id)}
                  <select {id} bind:value={profile!.options.ex_threads}><option value={null}>Automatic</option>{#each [0, 1, 3, 5, 7] as c}<option value={c}>{c}</option>{/each}</select>
                {/snippet}
              </Field>
            </div>
            <div class="flags">
              {#each flags as f}<Switch checked={!!profile.options[f.k]} onchange={(v) => ((profile!.options as any)[f.k] = v)} label={f.label} hint={f.hint} />{/each}
            </div>
            <Field label="Additional parameters" hint="Added to the command line as typed.">
              {#snippet children(id)}<input {id} class="mono" bind:value={profile!.additional_parameters} placeholder="-skipIntro -noLogs" spellcheck="false" />{/snippet}
            </Field>
          </Section>
          <Section title="When the game starts">
            <SegmentedControl bind:value={cfg.at_launch} options={[{ value: "nothing", label: "Keep ArmaXSync open" }, { value: "minimize", label: "Minimize" }, { value: "close", label: "Close" }]} />
          </Section>
        {:else if section === "apps"}
          <Section title="External applications" icon={AppWindow} description="Started together with the game, e.g. TeamSpeak or a VoIP plugin helper.">
            {#each cfg.external_apps as a, i}
              <div class="appcard card raised">
                <div class="row">
                  <Switch bind:checked={a.enabled} />
                  <input bind:value={a.name} placeholder="Name" style="max-width:240px" aria-label="Name" />
                  <div class="spacer"></div>
                  <button class="btn ghost icon sm" aria-label={`Remove ${a.name}`} onclick={() => cfg!.external_apps.splice(i, 1)}><X size={15} /></button>
                </div>
                <div class="form-grid">
                  <Field label="Program">{#snippet children(id)}<PathInput {id} bind:value={a.path} browse={() => pickFile("Program")} />{/snippet}</Field>
                  <Field label="Parameters">{#snippet children(id)}<input {id} class="mono" bind:value={a.parameters} />{/snippet}</Field>
                </div>
              </div>
            {:else}
              <p class="muted small">No external applications.</p>
            {/each}
            <div><Button size="sm" icon={Plus} onclick={() => cfg!.external_apps.push({ name: "", path: "", parameters: "", enabled: true })}>Add application</Button></div>
          </Section>
        {:else if section === "downloads"}
          <Section title="Downloads" icon={Download}>
            <Field label="Default download folder" hint="New repositories download into a sub-folder here.">
              {#snippet children(id)}<PathInput {id} bind:value={cfg!.default_destination} browse={() => pickFolder("Default download folder")} />{/snippet}
            </Field>
            <div class="form-grid">
              <Field label="Check for repository updates" hint="While ArmaXSync is open.">
                {#snippet children(id)}
                  <select {id} bind:value={cfg!.check_interval_minutes}>
                    <option value={0}>Only at startup</option>
                    {#each [15, 30, 60, 120, 360] as m}<option value={m}>Every {m < 60 ? `${m} minutes` : `${m / 60} hour${m > 60 ? "s" : ""}`}</option>{/each}
                  </select>
                {/snippet}
              </Field>
              <Field label="Speed limit (MB/s)" hint="All downloads together. 0 = unlimited.">
                {#snippet children(id)}<input {id} type="number" min="0" step="0.5" bind:value={speedMb} oninput={() => (cfg!.max_speed = Math.round((speedMb || 0) * 1e6))} />{/snippet}
              </Field>
              <Field label="Parallel ranges above (MB)" hint="Large files download in parallel parts. 0 = off.">
                {#snippet children(id)}<input {id} type="number" min="0" bind:value={segMb} oninput={() => (cfg!.segment_threshold = Math.round((segMb || 0) * 1e6))} />{/snippet}
              </Field>
              <Field label="Proxy" hint="http://, https:// or socks5:// URL.">
                {#snippet children(id)}<input {id} class="mono" bind:value={cfg!.proxy} placeholder="socks5://127.0.0.1:1080" />{/snippet}
              </Field>
            </div>
          </Section>
        {:else if section === "updates"}
          <Section title="ArmaXSync updates" icon={ArrowUpCircle}>
            <div class="row">
              <span>Version <b>{app.state!.version}</b></span>
              {#if app.update}<Badge tone="accent">{app.update.version} available</Badge>{:else}<Badge tone="ok">Up to date</Badge>{/if}
              <div class="spacer"></div>
              <Button size="sm" icon={RefreshCw} loading={app.updateChecking} onclick={() => checkForUpdate(false)}>Check now</Button>
              <Button size="sm" variant="ghost" icon={ExternalLink} onclick={() => openUrl(`https://github.com/${app.state!.release_repo}/releases`)}>Releases</Button>
            </div>
            {#if app.state!.install_kind === "manual"}
              <Alert tone="info">This copy was installed by a package manager or runs in portable mode: new versions are announced, but you install them yourself.</Alert>
            {/if}
            <Switch bind:checked={cfg.updates.check} label="Check for updates" hint="At startup and every 6 hours." />
            <Switch bind:checked={cfg.updates.auto_install} disabled={!cfg.updates.check || app.state!.install_kind !== "auto"} label="Install updates automatically" hint="Never while downloading or playing." />
            {#if cfg.updates.skipped}
              <div class="row"><span class="small muted">Skipped version {cfg.updates.skipped}</span><Button size="sm" variant="ghost" onclick={() => (cfg!.updates.skipped = null)}>Un-skip</Button></div>
            {/if}
          </Section>
        {:else if section === "appearance"}
          <Section title="Appearance" icon={Palette}>
            <Field label="Theme">
              {#snippet children()}<ThemeToggle size="md" onchange={(t) => (cfg!.ui.theme = t)} />{/snippet}
            </Field>
            <Switch bind:checked={cfg.ui.sidebar_collapsed} label="Compact sidebar" />
            <Switch bind:checked={cfg.ui.admin_tools} label="Repository hosting tools" hint="Show the Hosting tab on repositories, to build or push a repository you administer." />
          </Section>
        {:else if section === "about"}
          <Section title="About" icon={Info}>
            <div class="kv">
              <span>Version</span><span>{app.state!.version}{app.state!.portable ? " (portable)" : ""}</span>
              <span>Data folder</span><span class="row tight"><code>{app.state!.data_dir}</code><Button size="sm" variant="ghost" icon={FolderOpen} onclick={() => openPath(app.state!.data_dir)}>Open</Button></span>
              <span>Arma 3</span><span>{app.state!.game.arma3_dir ?? "not detected"}{app.state!.game.steam ? " · Steam" : ""}</span>
              <span>Workshop</span><span>{app.state!.game.workshop_dirs.join(", ") || "–"}</span>
              <span>Source</span><span><button class="btn ghost sm" onclick={() => openUrl(`https://github.com/${app.state!.release_repo}`)}><ExternalLink size={13} />{app.state!.release_repo}</button></span>
            </div>
            <Button variant="ghost" size="sm" onclick={async () => { await call("save_config", { config: { ...$state.snapshot(app.state!.config), ui: { ...app.state!.config.ui, onboarded: false } } }); await refresh(); }}>Run the setup again</Button>
          </Section>
        {/if}

        <SaveBar {dirty} {saving} onsave={save} ondiscard={discard} />
      </div>
    </div>
  {/if}
</div>

<style>
  .layout { display: grid; grid-template-columns: 200px minmax(0, 1fr); gap: 24px; align-items: start; }
  @media (max-width: 960px) { .layout { grid-template-columns: 1fr; } .subnav { flex-direction: row !important; flex-wrap: wrap; position: static !important; } }
  .subnav { display: flex; flex-direction: column; gap: 2px; position: sticky; top: 0; }
  .subnav button {
    display: flex; align-items: center; gap: 10px; padding: 8px 10px; border: 0; border-radius: var(--r); background: none;
    color: var(--fg-2); font: inherit; font-size: 13.5px; font-weight: 550; cursor: pointer; text-align: left;
  }
  .subnav button:hover { background: var(--surface-2); color: var(--fg); }
  .subnav button.active { background: var(--surface-2); color: var(--fg); box-shadow: inset 2px 0 0 var(--accent); }
  .subnav button.active :global(svg) { color: var(--accent); }
  .content { max-width: 860px; }
  .profbar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; flex-wrap: wrap; }
  .flags { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 14px 18px; }
  .appcard { display: flex; flex-direction: column; gap: 10px; padding: 12px 14px; }
</style>
