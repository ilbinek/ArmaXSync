<script lang="ts">
  import { onDestroy } from "svelte";
  import { PageHeader, Tabs, Section, Button, Field, PathInput, Switch, SegmentedControl, TagListInput, CopyField, Alert, EmptyState, copyText, toast, toastError, errorText } from "@axs/kit";
  import { ScrollText, KeyRound, ServerCog, History, FileSearch, FolderOpen, Pause, Play, Copy, Eraser, Search, Layers } from "@lucide/svelte";
  import { call, pickFolder, pickFile, selectedAddons } from "../api";
  import { app, refresh, loadProfile } from "../store.svelte";

  let tab = $state<string>();

  // --------------------------------------------------------------- RPT ----
  let rptPath = $state("");
  let rptText = $state("");
  let rptOffset = 0;
  let follow = $state(true);
  let level = $state<"all" | "err" | "warn">("all");
  let q = $state("");
  let rptTimer: ReturnType<typeof setInterval> | undefined;
  let rptEl = $state<HTMLElement>();

  async function openRpt(path?: string | null) {
    const p = path ?? (await call<string | null>("latest_rpt", { proton: !!app.profile?.proton }).catch(() => null));
    if (!p) return toast("No RPT log found. Start the game once first.", "warn");
    rptPath = p;
    rptText = "";
    rptOffset = 0;
    clearInterval(rptTimer);
    await poll();
    rptTimer = setInterval(() => follow && poll(), 1000);
  }
  async function poll() {
    try {
      const c = await call<{ text: string; offset: number }>("read_text_from", { path: rptPath, offset: rptOffset });
      if (c.offset < rptOffset) rptText = "";
      if (c.text) {
        rptText = (rptText + c.text).slice(-2_000_000);
        if (follow) queueMicrotask(() => rptEl?.scrollTo(0, rptEl.scrollHeight));
      }
      rptOffset = c.offset;
    } catch (e) {
      clearInterval(rptTimer);
      toastError(e, "Stopped following the log");
    }
  }
  onDestroy(() => clearInterval(rptTimer));

  const classify = (l: string) => (/\berror\b|exception|failed|cannot|missing/i.test(l) ? "err" : /\bwarning\b/i.test(l) ? "warn" : "");
  const lines = $derived.by(() => {
    const all = rptText.split("\n").map((t, i) => ({ i, t, c: classify(t) }));
    const ql = q.toLowerCase();
    return all.filter((l) => (level === "all" || l.c === level) && (!ql || l.t.toLowerCase().includes(ql))).slice(-4000);
  });
  const counts = $derived.by(() => {
    let e = 0, w = 0;
    for (const l of rptText.split("\n")) {
      const c = classify(l);
      if (c === "err") e++;
      else if (c === "warn") w++;
    }
    return { e, w };
  });

  // -------------------------------------------------------------- keys ----
  let keySrc = $state("");
  let keyDst = $state("");
  let keys = $state<string[] | null>(null);
  let keyBusy = $state(false);
  async function extract() {
    keyBusy = true;
    try {
      keys = await call<string[]>("extract_keys", { src: keySrc, dst: keyDst });
    } catch (e) {
      toastError(e);
    } finally {
      keyBusy = false;
    }
  }

  // ------------------------------------------------------- server helper ----
  let srvDir = $state("");
  let srvMods = $state<string[]>([]);
  let srvServerMods = $state<string[]>([]);
  let srvCopy = $state(false);
  let srvLower = $state(false);
  let srvBusy = $state(false);
  let srvOut = $state<{ mods: string[]; server_mods: string[]; keys: string[]; parameters: string[] } | null>(null);
  let srvErr = $state<string | null>(null);
  async function fillFromProfile() {
    await loadProfile();
    srvMods = selectedAddons(app.profile!.groups);
  }
  async function serverHelper() {
    srvBusy = true;
    srvErr = null;
    try {
      srvOut = await call("server_helper", { params: { profile: $state.snapshot(app.profile), server_dir: srvDir, target: null, mods: srvMods, server_mods: srvServerMods, copy: srvCopy, lowercase: srvLower } });
    } catch (e) {
      srvErr = errorText(e);
    } finally {
      srvBusy = false;
    }
  }

  // --------------------------------------------------------------- import ----
  let legacyDir = $state("");
  let overwrite = $state(false);
  let importBusy = $state(false);
  let imp = $state<any>(null);
  async function importLegacy() {
    importBusy = true;
    try {
      imp = await call("import_legacy", { path: legacyDir, overwrite });
      await refresh();
      await loadProfile();
    } catch (e) {
      toastError(e);
    } finally {
      importBusy = false;
    }
  }
</script>

<div class="page">
  <PageHeader title="Tools" subtitle="Logs, keys, dedicated servers and migration." />
  <Tabs
    bind:value={tab}
    param="tab"
    items={[
      { id: "rpt", label: "RPT log", icon: ScrollText },
      { id: "keys", label: "Bikeys", icon: KeyRound },
      { id: "server", label: "Dedicated server", icon: ServerCog },
      { id: "import", label: "Import ArmA3Sync", icon: History },
    ]}
  />

  {#if tab === "rpt"}
    <section class="card flush rpt">
      <div class="tb">
        <Button size="sm" variant="primary" icon={FileSearch} onclick={() => openRpt()}>Newest log</Button>
        <Button size="sm" icon={FolderOpen} onclick={async () => openRpt(await pickFile("Select RPT", ["rpt", "log", "txt"]))}>Open…</Button>
        {#if rptPath}
          <Button size="sm" variant="ghost" icon={follow ? Pause : Play} onclick={() => (follow = !follow)}>{follow ? "Pause" : "Follow"}</Button>
          <div class="spacer"></div>
          <SegmentedControl size="sm" bind:value={level} options={[{ value: "all", label: "All" }, { value: "err", label: `Errors ${counts.e}` }, { value: "warn", label: `Warnings ${counts.w}` }]} />
          <div class="search"><Search size={13} /><input type="search" placeholder="Search…" bind:value={q} /></div>
          <Button size="sm" variant="ghost" icon={Copy} tip="Copy shown lines" onclick={async () => (await copyText(lines.map((l) => l.t).join("\n"))) && toast("Copied")} />
          <Button size="sm" variant="ghost" icon={Eraser} tip="Clear view" onclick={() => (rptText = "")} />
        {/if}
      </div>
      {#if rptPath}
        <div class="path xs subtle mono ellipsis" title={rptPath}>{rptPath}{follow ? " · following" : " · paused"}</div>
        <div class="log rptlog" bind:this={rptEl}>
          {#each lines as l (l.i)}<div class="ln {l.c ? 'l-' + l.c : ''}">{l.t || " "}</div>{:else}<span class="subtle">No matching lines.</span>{/each}
        </div>
      {:else}
        <EmptyState icon={ScrollText} title="Game log viewer" description="Open the newest Arma 3 RPT log to watch it live, with errors and warnings highlighted." compact>
          <Button variant="primary" icon={FileSearch} onclick={() => openRpt()}>Open newest log</Button>
        </EmptyState>
      {/if}
    </section>
  {:else if tab === "keys"}
    <Section title="Extract bikeys" icon={KeyRound} description="Copies every .bikey found below the source folder into the target folder, e.g. a dedicated server's keys folder.">
      <div class="form-grid">
        <Field label="Source (mods folder)">{#snippet children(id)}<PathInput {id} bind:value={keySrc} browse={() => pickFolder("Mods folder")} />{/snippet}</Field>
        <Field label="Target (keys folder)">{#snippet children(id)}<PathInput {id} bind:value={keyDst} browse={() => pickFolder("Keys folder")} />{/snippet}</Field>
      </div>
      {#if keys}
        <Alert tone={keys.length ? "ok" : "warn"} title={keys.length ? `${keys.length} key${keys.length > 1 ? "s" : ""} copied` : "No keys found"}>
          {#if keys.length}<div class="chips" style="margin-top:6px">{#each keys as k}<span class="chip mono">{k}</span>{/each}</div>{/if}
        </Alert>
      {/if}
      {#snippet footer()}<Button variant="primary" icon={KeyRound} loading={keyBusy} disabled={!keySrc || !keyDst} onclick={extract}>Extract keys</Button>{/snippet}
    </Section>
  {:else if tab === "server"}
    <Section title="Prepare a dedicated server" icon={ServerCog} description="Links (or copies) mods into <server>/mods, copies their keys into <server>/keys and builds the -mod / -serverMod parameters.">
      <Field label="Server folder" required>{#snippet children(id)}<PathInput {id} bind:value={srvDir} browse={() => pickFolder("Server folder")} />{/snippet}</Field>
      <Field label="Mods (-mod)" required>
        {#snippet aside()}<button class="btn ghost sm" onclick={fillFromProfile}><Layers size={13} />From profile</button>{/snippet}
        {#snippet children(id)}<TagListInput {id} bind:values={srvMods} placeholder="@CBA_A3" />{/snippet}
      </Field>
      <Field label="Server-only mods (-serverMod)">{#snippet children(id)}<TagListInput {id} bind:values={srvServerMods} placeholder="@server_admin_tools" />{/snippet}</Field>
      <div class="row loose">
        <Switch bind:checked={srvCopy} label="Copy instead of linking" hint="Use when the server runs on another drive." />
        <Switch bind:checked={srvLower} label="Lowercase names" hint="For Linux servers." />
      </div>
      {#if srvErr}<Alert tone="err">{srvErr}</Alert>{/if}
      {#if srvOut}
        <Alert tone="ok" title="Server prepared">{srvOut.mods.length + srvOut.server_mods.length} mod(s) linked, {srvOut.keys.length} key(s) copied.</Alert>
        <CopyField label="Start parameters" value={srvOut.parameters.join(" ")} />
      {/if}
      {#snippet footer()}<Button variant="primary" icon={ServerCog} loading={srvBusy} disabled={!srvDir || !srvMods.length} onclick={serverHelper}>Prepare server</Button>{/snippet}
    </Section>
  {:else if tab === "import"}
    <Section title="Import from ArmA3Sync" icon={History} description="Repositories (with saved logins), launcher profiles, favorite servers and external applications.">
      <Field label="ArmA3Sync installation folder">{#snippet children(id)}<PathInput {id} bind:value={legacyDir} placeholder="C:\ArmA3Sync" browse={() => pickFolder("ArmA3Sync folder")} />{/snippet}</Field>
      <Switch bind:checked={overwrite} label="Overwrite existing entries" hint="Replace repositories and profiles that have the same name." />
      {#if imp}
        <div class="kv">
          <span>Repositories</span><span>{imp.repositories.repositories.join(", ") || "none"}</span>
          <span>Profiles</span><span>{imp.profiles.join(", ") || "none"}</span>
          <span>Favorite servers</span><span>{imp.favorite_servers}</span>
          <span>External apps</span><span>{imp.external_apps}</span>
        </div>
        {#if imp.repositories.skipped.length || imp.errors.length}
          <Alert tone="warn" title="Skipped">
            <ul class="skip">{#each [...imp.repositories.skipped, ...imp.errors] as [n, why]}<li><b>{n}</b>: {why}</li>{/each}</ul>
          </Alert>
        {/if}
      {/if}
      {#snippet footer()}<Button variant="primary" icon={History} loading={importBusy} disabled={!legacyDir} onclick={importLegacy}>Import</Button>{/snippet}
    </Section>
  {/if}
</div>

<style>
  .rpt { display: flex; flex-direction: column; }
  .tb { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
  .search { position: relative; width: 180px; }
  .search :global(svg) { position: absolute; left: 8px; top: 50%; transform: translateY(-50%); color: var(--fg-3); }
  .search input { height: 26px; padding-left: 26px; font-size: 12.5px; }
  .path { padding: 6px 12px; border-bottom: 1px solid var(--line); }
  .rptlog { border: 0; border-radius: 0; max-height: none; height: calc(100vh - 310px); min-height: 280px; white-space: pre; overflow: auto; }
  .ln { min-height: 1.55em; }
  .ln.l-err { color: var(--err); background: var(--err-soft); }
  .ln.l-warn { color: var(--warn); }
  .chip.mono { font-family: var(--mono); font-size: 12px; }
  .skip { margin: 4px 0 0; padding-left: 18px; }
</style>
