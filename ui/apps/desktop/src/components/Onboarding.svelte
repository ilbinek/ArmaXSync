<script lang="ts">
  import { onMount } from "svelte";
  import { Logo, Button, Field, PathInput, Alert, toast, errorText } from "@axs/kit";
  import { Gamepad2, FolderDown, History, Database, Check, ArrowRight } from "@lucide/svelte";
  import { call, pickFile, pickFolder } from "../api";
  import { app, refresh, loadProfile, saveConfig, saveProfile } from "../store.svelte";

  let { onadd }: { onadd: () => void } = $props();

  let exe = $state("");
  let dest = $state("");
  let legacy = $state("");
  let importing = $state(false);
  let imported = $state<string | null>(null);
  let busy = $state(false);

  onMount(() => {
    const g = app.state!.game;
    exe = app.profile?.executable ?? (g.arma3_dir ? `${g.arma3_dir}${g.linux ? "/" : "\\"}arma3_x64.exe` : "");
    dest = app.state!.config.default_destination ?? "";
  });

  async function importLegacy() {
    importing = true;
    try {
      const r: any = await call("import_legacy", { path: legacy, overwrite: false });
      await refresh();
      await loadProfile();
      imported = `${r.repositories.repositories.length} repositories, ${r.profiles.length} profiles, ${r.favorite_servers} servers imported`;
    } catch (e) {
      toast(errorText(e), "err");
    } finally {
      importing = false;
    }
  }

  async function finish(add: boolean) {
    busy = true;
    try {
      if (app.profile && exe && exe !== app.profile.executable) await saveProfile({ ...$state.snapshot(app.profile), executable: exe });
      await saveConfig({ default_destination: dest || null, ui: { ...app.state!.config.ui, onboarded: true } });
      if (add) onadd();
    } catch (e) {
      toast(errorText(e), "err");
    } finally {
      busy = false;
    }
  }
</script>

<div class="ob gridbg">
  <div class="inner">
    <div class="hero">
      <Logo size={52} />
      <div>
        <div class="eyebrow">Welcome</div>
        <h1>Set up ArmaXSync</h1>
        <p class="muted">Three quick steps, then add your community's repository. You can change everything later in Settings.</p>
      </div>
    </div>

    <ol class="steps">
      <li class="card">
        <div class="num" class:done={!!exe}>{#if exe}<Check size={16} />{:else}1{/if}</div>
        <div class="stack tight grow">
          <h2><Gamepad2 size={17} />Arma 3</h2>
          {#if app.state?.game.arma3_dir}
            <p class="small muted">Found in <code>{app.state.game.arma3_dir}</code>{app.state.game.steam ? " (Steam)" : ""}.</p>
          {:else}
            <p class="small muted">Arma 3 was not found automatically. Select the game executable.</p>
          {/if}
          <Field label="Game executable">
            {#snippet children(id)}<PathInput {id} bind:value={exe} placeholder="arma3_x64.exe" browse={() => pickFile("Arma 3 executable", ["exe"])} />{/snippet}
          </Field>
        </div>
      </li>
      <li class="card">
        <div class="num" class:done={!!dest}>{#if dest}<Check size={16} />{:else}2{/if}</div>
        <div class="stack tight grow">
          <h2><FolderDown size={17} />Download folder</h2>
          <p class="small muted">New repositories download into a sub-folder here. Pick a drive with plenty of space: modsets are often 50–150 GB.</p>
          <Field label="Default download folder">
            {#snippet children(id)}<PathInput {id} bind:value={dest} placeholder="D:\ArmaMods" browse={() => pickFolder("Default download folder")} />{/snippet}
          </Field>
        </div>
      </li>
      <li class="card">
        <div class="num" class:done={!!imported}>{#if imported}<Check size={16} />{:else}3{/if}</div>
        <div class="stack tight grow">
          <h2><History size={17} />Coming from ArmA3Sync? <span class="badge plain">optional</span></h2>
          <p class="small muted">Import repositories, launcher profiles and favorite servers from an ArmA3Sync installation.</p>
          {#if imported}
            <Alert tone="ok">{imported}</Alert>
          {:else}
            <div class="row nowrap">
              <div class="grow"><PathInput bind:value={legacy} placeholder="C:\ArmA3Sync" browse={() => pickFolder("ArmA3Sync installation folder")} /></div>
              <Button loading={importing} disabled={!legacy} onclick={importLegacy}>Import</Button>
            </div>
          {/if}
        </div>
      </li>
    </ol>

    <div class="row end">
      <Button variant="ghost" disabled={busy} onclick={() => finish(false)}>Skip for now</Button>
      <Button variant="primary" size="lg" icon={Database} iconRight={ArrowRight} loading={busy} onclick={() => finish(true)}>Add a repository</Button>
    </div>
  </div>
</div>

<style>
  .ob { min-height: 100vh; display: grid; place-items: center; padding: 40px 24px; overflow: auto; }
  .inner { width: min(760px, 100%); display: flex; flex-direction: column; gap: 22px; }
  .hero { display: flex; gap: 18px; align-items: center; }
  .hero h1 { font-size: 34px; margin: 2px 0 6px; }
  .hero p { margin: 0; max-width: 60ch; }
  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 12px; }
  .steps li { display: flex; gap: 16px; align-items: flex-start; }
  .steps h2 { display: flex; align-items: center; gap: 8px; margin: 0; }
  .steps h2 :global(svg) { color: var(--accent-text); }
  .steps p { margin: 0; }
  .num {
    flex-shrink: 0; width: 32px; height: 32px; border-radius: 50%; display: grid; place-items: center;
    border: 1px solid var(--line-strong); font-family: var(--font-display); font-weight: 700; font-size: 16px; color: var(--fg-2);
  }
  .num.done { background: var(--ok-soft); border-color: transparent; color: var(--ok); }
</style>
