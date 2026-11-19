<script lang="ts">
  import { onMount } from "svelte";
  import { Modal, Button, Field, PasswordInput, PathInput, Switch, Alert, toast, errorText } from "@axs/kit";
  import { Link, ArrowLeft, ChevronDown } from "@lucide/svelte";
  import { call, pickFolder, type RepoConfig } from "../api";
  import { app, refresh, startSync, loadProfile } from "../store.svelte";

  let { url = "", onclose }: { url?: string; onclose: (name?: string) => void } = $props();
  let open = $state(true);
  // svelte-ignore state_referenced_locally
  let input = $state(url);
  let showLogin = $state(false);
  let login = $state("");
  let password = $state("");
  let resolved = $state<RepoConfig | null>(null);
  let busy = $state(false);
  let error = $state("");
  let syncNow = $state(true);
  let addModset = $state(true);

  const nameTaken = $derived(!!resolved && !!app.state?.repos.some((r) => r.name.toLowerCase() === resolved!.name.trim().toLowerCase()));

  function close(name?: string) {
    open = false;
    onclose(name);
  }

  function defaultDest(name: string) {
    const base = app.state?.config.default_destination;
    if (!base) return null;
    const sep = base.includes("\\") ? "\\" : "/";
    return `${base.replace(/[\\/]+$/, "")}${sep}${name.replace(/[^\w.-]+/g, "")}`;
  }

  async function resolve() {
    error = "";
    busy = true;
    try {
      const r = await call<RepoConfig>("repo_resolve", { url: input.trim(), login: login || null, password: password || null });
      r.destination ??= defaultDest(r.name);
      resolved = r;
    } catch (e) {
      error = errorText(e);
      if (/401|auth|login|forbidden/i.test(error)) showLogin = true;
    } finally {
      busy = false;
    }
  }

  // armaxsync:// links arrive prefilled: connect right away.
  onMount(() => {
    if (url.trim()) resolve();
  });

  async function save() {
    if (!resolved || nameTaken) return;
    busy = true;
    error = "";
    try {
      const name = resolved.name.trim();
      await call("repo_save", { repo: { ...resolved, name }, original: null });
      await refresh();
      if (addModset) {
        try {
          await call("apply_modset", { profile: app.state!.active_profile, repo: name, event: null });
          await loadProfile();
        } catch (e) {
          toast(`Modset not added: ${errorText(e)}`, "warn");
        }
      }
      if (syncNow) await startSync(name);
      toast(`${name} added`, "ok", { detail: syncNow ? "Downloading now" : undefined });
      close(name);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="Add repository" description={resolved ? "Step 2 of 2 · Where to download" : "Step 1 of 2 · Connect"} persistent={busy} onclose={() => onclose()}>
  {#if !resolved}
    <form class="stack" onsubmit={(e) => (e.preventDefault(), resolve())}>
      <Field label="Repository link" hint="Paste the auto-config URL, repository URL or armaxsync:// link from your community.">
        {#snippet children(id)}
          <input {id} bind:value={input} placeholder="https://mods.example.com/r/main/.a3s/autoconfig" spellcheck="false" data-autofocus />
        {/snippet}
      </Field>
      <button type="button" class="disclose" onclick={() => (showLogin = !showLogin)} aria-expanded={showLogin}>
        <ChevronDown size={14} style={showLogin ? "" : "transform:rotate(-90deg)"} />Login for private repositories
      </button>
      {#if showLogin}
        <div class="form-grid">
          <Field label="Login">{#snippet children(id)}<input {id} bind:value={login} autocomplete="username" />{/snippet}</Field>
          <Field label="Password">{#snippet children(id)}<PasswordInput {id} bind:value={password} autocomplete="current-password" />{/snippet}</Field>
        </div>
      {/if}
    </form>
  {:else}
    <div class="repo-head">
      <Link size={16} />
      <span class="mono small ellipsis grow" title={resolved.url}>{resolved.url}</span>
      <span class="badge">{resolved.game}</span>
    </div>
    <Field label="Name" error={nameTaken ? "You already have a repository with this name" : null}>
      {#snippet children(id)}<input {id} bind:value={resolved!.name} aria-invalid={nameTaken} />{/snippet}
    </Field>
    <Field label="Download to" required hint="Addons are downloaded into this folder. It is searched for addons automatically.">
      {#snippet children(id)}<PathInput {id} bind:value={resolved!.destination} placeholder="e.g. D:\ArmaMods\Community" browse={() => pickFolder("Download destination")} />{/snippet}
    </Field>
    <div class="stack tight opts">
      <Switch bind:checked={syncNow} label="Download now" hint="Start synchronizing as soon as the repository is added." />
      <Switch bind:checked={addModset} label={`Add its modset to profile “${app.state?.active_profile}”`} hint="So the repository's addons load when you start the game." />
      <Switch bind:checked={resolved.notify} label="Notify me about updates" />
      <Switch bind:checked={resolved.auto} label="Download updates automatically" />
    </div>
  {/if}
  {#if error}<Alert tone="err" title="Could not connect">{error}</Alert>{/if}
  {#snippet footer()}
    {#if !resolved}
      <Button onclick={() => close()}>Cancel</Button>
      <Button variant="primary" loading={busy} disabled={!input.trim()} onclick={resolve}>Continue</Button>
    {:else}
      <Button variant="ghost" icon={ArrowLeft} onclick={() => (resolved = null)} disabled={busy}>Back</Button>
      <div class="spacer"></div>
      <Button variant="primary" loading={busy} disabled={!resolved.destination || !resolved.name.trim() || nameTaken} onclick={save}>Add repository</Button>
    {/if}
  {/snippet}
</Modal>

<style>
  .disclose { display: inline-flex; align-items: center; gap: 6px; background: none; border: 0; padding: 0; color: var(--fg-2); font: inherit; font-size: 13px; cursor: pointer; align-self: flex-start; }
  .disclose:hover { color: var(--fg); }
  .repo-head { display: flex; align-items: center; gap: 8px; padding: 8px 10px; background: var(--bg); border: 1px solid var(--line); border-radius: var(--r); }
  .repo-head :global(svg) { color: var(--accent); }
  .opts { padding-top: 4px; }
</style>
