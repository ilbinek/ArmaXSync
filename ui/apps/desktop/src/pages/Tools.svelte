<script lang="ts">
  import { onDestroy } from "svelte";
  import { toast } from "@axs/kit";
  import { call, pickFolder, pickFile, selectedAddons, type Profile } from "../api";
  import { app, refresh } from "../store.svelte";

  let tab = $state("rpt");

  // RPT viewer
  let rptPath = $state("");
  let rptText = $state("");
  let rptOffset = 0;
  let follow = $state(true);
  let rptTimer: any;
  let rptEl: HTMLElement | undefined = $state();
  async function openRpt(path?: string | null) {
    const p = path ?? (await call<string | null>("latest_rpt", { proton: false }));
    if (!p) return toast("No RPT file found", "err");
    rptPath = p;
    rptText = "";
    rptOffset = 0;
    clearInterval(rptTimer);
    await poll();
    rptTimer = setInterval(() => follow && poll(), 1000);
  }
  async function poll() {
    const c: any = await call("read_text_from", { path: rptPath, offset: rptOffset }).catch(() => null);
    if (!c) return;
    if (c.offset < rptOffset) rptText = "";
    if (c.text) {
      rptText = (rptText + c.text).slice(-2_000_000);
      queueMicrotask(() => rptEl?.scrollTo(0, rptEl.scrollHeight));
    }
    rptOffset = c.offset;
  }
  onDestroy(() => clearInterval(rptTimer));

  // Bikeys
  let keySrc = $state("");
  let keyDst = $state("");
  async function extract() {
    try {
      const k: string[] = await call("extract_keys", { src: keySrc, dst: keyDst });
      toast(`${k.length} key(s) copied`);
    } catch (e: any) {
      toast(String(e), "err");
    }
  }

  // Server helper
  let srvDir = $state("");
  let srvMods = $state("");
  let srvServerMods = $state("");
  let srvCopy = $state(false);
  let srvLower = $state(false);
  let srvOut = $state("");
  async function fillFromProfile() {
    const p: Profile = await call("profile_get", { name: app.state!.active_profile });
    srvMods = selectedAddons(p.groups).join(", ");
  }
  async function serverHelper() {
    try {
      const profile = await call<Profile>("profile_get", { name: app.state!.active_profile });
      const list = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);
      const r: any = await call("server_helper", { params: { profile, server_dir: srvDir, target: null, mods: list(srvMods), server_mods: list(srvServerMods), copy: srvCopy, lowercase: srvLower } });
      srvOut = `${r.mods.length + r.server_mods.length} mod(s) prepared, ${r.keys.length} key(s) copied.\n\nStart parameters:\n${r.parameters.join(" ")}`;
    } catch (e: any) {
      srvOut = String(e);
    }
  }

  // Legacy import
  let legacyDir = $state("");
  let importOut = $state("");
  async function importLegacy() {
    try {
      const r: any = await call("import_legacy", { path: legacyDir, overwrite: false });
      importOut = `Repositories: ${r.repositories.repositories.join(", ") || "none"}\nProfiles: ${r.profiles.join(", ") || "none"}\nFavorite servers: ${r.favorite_servers}\nExternal applications: ${r.external_apps}` +
        (r.errors.length || r.repositories.skipped.length ? `\nSkipped: ${[...r.repositories.skipped, ...r.errors].map((x: any) => x.join(": ")).join("; ")}` : "");
      await refresh();
    } catch (e: any) {
      importOut = String(e);
    }
  }
</script>

<h1>Tools</h1>
<div class="tabs" style="margin-top:10px">
  {#each [["rpt", "RPT viewer"], ["keys", "Bikey extractor"], ["server", "Server helper"], ["import", "Import ArmA3Sync"]] as [id, l]}
    <button class:active={tab === id} onclick={() => (tab = id)}>{l}</button>
  {/each}
</div>

{#if tab === "rpt"}
  <div class="card stack">
    <div class="row">
      <button class="btn primary" onclick={() => openRpt()}>Open newest RPT</button>
      <button class="btn" onclick={async () => openRpt(await pickFile("Select RPT", ["rpt", "log", "txt"]))}>Open file…</button>
      <label class="check"><input type="checkbox" bind:checked={follow} /> Follow</label>
      <span class="small muted">{rptPath}</span>
    </div>
    <div class="log" style="max-height:60vh;min-height:300px" bind:this={rptEl}>{rptText || "No file opened."}</div>
  </div>
{:else if tab === "keys"}
  <div class="card stack">
    <div class="small muted">Copies every *.bikey found below the source folder into the target folder (e.g. a dedicated server's keys folder).</div>
    <label class="field">Source <div class="row"><input bind:value={keySrc} style="flex:1" /><button class="btn" onclick={async () => (keySrc = (await pickFolder()) ?? keySrc)}>Browse…</button></div></label>
    <label class="field">Target <div class="row"><input bind:value={keyDst} style="flex:1" /><button class="btn" onclick={async () => (keyDst = (await pickFolder()) ?? keyDst)}>Browse…</button></div></label>
    <div class="row end"><button class="btn primary" disabled={!keySrc || !keyDst} onclick={extract}>Extract keys</button></div>
  </div>
{:else if tab === "server"}
  <div class="card stack">
    <div class="small muted">Prepares a dedicated server folder: links (or copies) the mods into &lt;server&gt;/mods, copies their keys into &lt;server&gt;/keys and prints the -mod / -serverMod parameters.</div>
    <label class="field">Server folder <div class="row"><input bind:value={srvDir} style="flex:1" /><button class="btn" onclick={async () => (srvDir = (await pickFolder()) ?? srvDir)}>Browse…</button></div></label>
    <label class="field">Mods (-mod), comma separated <div class="row"><input bind:value={srvMods} style="flex:1" /><button class="btn" onclick={fillFromProfile}>From profile</button></div></label>
    <label class="field">Server-only mods (-serverMod) <input bind:value={srvServerMods} /></label>
    <div class="row">
      <label class="check"><input type="checkbox" bind:checked={srvCopy} /> Copy instead of linking</label>
      <label class="check"><input type="checkbox" bind:checked={srvLower} /> Lowercase names (Linux servers)</label>
    </div>
    <div class="row end"><button class="btn primary" disabled={!srvDir || !srvMods} onclick={serverHelper}>Prepare server</button></div>
    {#if srvOut}<div class="log">{srvOut}</div>{/if}
  </div>
{:else if tab === "import"}
  <div class="card stack">
    <div class="small muted">Imports repositories (including saved logins), launcher profiles, favorite servers and external applications from an ArmA3Sync installation folder.</div>
    <label class="field">ArmA3Sync folder <div class="row"><input bind:value={legacyDir} style="flex:1" /><button class="btn" onclick={async () => (legacyDir = (await pickFolder()) ?? legacyDir)}>Browse…</button></div></label>
    <div class="row end"><button class="btn primary" disabled={!legacyDir} onclick={importLegacy}>Import</button></div>
    {#if importOut}<div class="log">{importOut}</div>{/if}
  </div>
{/if}
