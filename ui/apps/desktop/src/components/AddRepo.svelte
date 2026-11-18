<script lang="ts">
  import { Modal, toast } from "@axs/kit";
  import { call, pickFolder, type RepoConfig } from "../api";
  import { refresh } from "../store.svelte";

  let { url = "", onclose }: { url?: string; onclose: (name?: string) => void } = $props();
  let open = $state(true);
  // svelte-ignore state_referenced_locally
  let input = $state(url);
  let login = $state("");
  let password = $state("");
  let resolved = $state<RepoConfig | null>(null);
  let busy = $state(false);
  let error = $state("");

  $effect(() => {
    if (!open) onclose();
  });

  async function resolve() {
    error = "";
    busy = true;
    try {
      resolved = await call<RepoConfig>("repo_resolve", { url: input.trim(), login: login || null, password: password || null });
    } catch (e: any) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function chooseDest() {
    const d = await pickFolder("Download destination");
    if (d && resolved) resolved.destination = d;
  }

  async function save() {
    if (!resolved) return;
    try {
      await call("repo_save", { repo: resolved, original: null });
      await refresh();
      toast(`Repository ${resolved.name} added`);
      const n = resolved.name;
      open = false;
      onclose(n);
    } catch (e: any) {
      error = String(e);
    }
  }
</script>

<Modal bind:open title="Add repository">
  {#if !resolved}
    <label class="field">Auto-config URL, repository URL or armaxsync:// link
      <input bind:value={input} placeholder="https://mods.example.com/r/main/.a3s/autoconfig" />
    </label>
    <details>
      <summary class="small muted">Login (private repositories)</summary>
      <div class="grid2" style="margin-top:8px">
        <label class="field">Login <input bind:value={login} /></label>
        <label class="field">Password <input type="password" bind:value={password} /></label>
      </div>
    </details>
  {:else}
    <div class="grid2">
      <label class="field">Name <input bind:value={resolved.name} /></label>
      <label class="field">Game <input value={resolved.game} disabled /></label>
    </div>
    <div class="small muted">{resolved.url}</div>
    <label class="field">Download to
      <div class="row"><input bind:value={resolved.destination} placeholder="e.g. D:\Arma3Mods" style="flex:1" /><button class="btn" onclick={chooseDest}>Browse…</button></div>
      <span class="hint">Addons are downloaded into this folder; it is added to your addon search folders automatically.</span>
    </label>
    <div class="row">
      <label class="check"><input type="checkbox" bind:checked={resolved.notify} /> Notify me about updates</label>
      <label class="check"><input type="checkbox" bind:checked={resolved.auto} /> Download updates automatically</label>
    </div>
  {/if}
  {#if error}<div class="alert err">{error}</div>{/if}
  {#snippet footer()}
    {#if !resolved}
      <button class="btn primary" disabled={!input.trim() || busy} onclick={resolve}>{busy ? "Connecting…" : "Continue"}</button>
    {:else}
      <button class="btn" onclick={() => (resolved = null)}>Back</button>
      <button class="btn primary" disabled={!resolved.destination} onclick={save}>Add repository</button>
    {/if}
  {/snippet}
</Modal>
