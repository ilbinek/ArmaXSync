<script lang="ts">
  import { Stat, Section, Button, Skeleton, EmptyState, CopyField, bytes, ago, toast, toastError } from "@axs/kit";
  import { Package, Server as ServerIcon, Star, CircleAlert, RotateCw, Layers } from "@lucide/svelte";
  import { call, type RepoConfig, type RepoInfo } from "../../api";
  import { app, saveConfig, loadProfile } from "../../store.svelte";

  let { repo, info, loading, error, onretry }: { repo: RepoConfig; info: RepoInfo | null; loading: boolean; error: string | null; onretry: () => void } = $props();

  let filter = $state("");
  const addons = $derived((info?.addons ?? []).filter((a) => a.toLowerCase().includes(filter.toLowerCase())));
  const favs = $derived(app.state?.config.favorite_servers ?? []);
  const isFav = (h: string, p: number) => favs.some((f) => f.host === h && f.port === p);

  async function addFav(s: RepoInfo["servers"][number]) {
    try {
      await saveConfig({ favorite_servers: [...favs, { name: s.name, host: s.host, port: s.port, password: s.password ?? null, modset: s.modset ?? repo.name, repository: repo.name }] });
      toast(`${s.name} added to your servers`);
    } catch (e) {
      toastError(e);
    }
  }

  async function addModset() {
    try {
      await call("apply_modset", { profile: app.state!.active_profile, repo: repo.name, event: null });
      await loadProfile();
      toast(`Modset ${repo.name} added to profile ${app.state!.active_profile}`, "ok", { action: { label: "Open modsets", run: () => (location.hash = "#/modsets") } });
    } catch (e) {
      toastError(e);
    }
  }
</script>

<div class="stack loose">
  <div class="grid3">
    <Stat label="Your revision" value={repo.state.revision ?? "–"} hint={repo.state.last_sync ? `Synced ${ago(repo.state.last_sync)}` : "Never synced"} />
    <Stat label="Remote revision" value={info?.revision ?? repo.state.remote_revision ?? "–"} tone={repo.state.status === "updated" ? "warn" : undefined} hint={info ? `${info.source} metadata` : undefined} />
    <Stat label="Size" value={info ? bytes(info.size) : "–"} hint={info ? `${info.files.toLocaleString()} files` : undefined} />
    <Stat label="Addons" value={info?.addons.length ?? "–"} hint={info ? `${info.events.length} events` : undefined} />
  </div>

  {#if error}
    <div class="card">
      <EmptyState icon={CircleAlert} title="Could not read the repository" description={error} compact>
        <Button icon={RotateCw} onclick={onretry}>Retry</Button>
      </EmptyState>
    </div>
  {/if}

  <div class="cols">
    <Section title="Addons" icon={Package} description="Folders this repository provides.">
      {#snippet actions()}<Button size="sm" icon={Layers} onclick={addModset}>Add modset to profile</Button>{/snippet}
      {#if loading && !info}
        <Skeleton lines={3} />
      {:else if info}
        {#if info.addons.length > 12}<input type="search" placeholder="Filter addons…" bind:value={filter} />{/if}
        <div class="chips">
          {#each addons as a}<span class="chip mono">{a}</span>{:else}<span class="muted small">No addons.</span>{/each}
        </div>
      {/if}
    </Section>

    <div class="stack">
      <Section title="Servers" icon={ServerIcon}>
        {#if loading && !info}
          <Skeleton lines={2} />
        {:else if info?.servers.length}
          <ul class="list">
            {#each info.servers as s}
              <li>
                <div class="grow">
                  <div class="strong small">{s.name}</div>
                  <div class="xs muted mono">{s.host}:{s.port}{s.modset ? ` · ${s.modset}` : ""}</div>
                </div>
                {#if isFav(s.host, s.port)}
                  <span class="xs ok-text">In your servers</span>
                {:else}
                  <Button size="sm" icon={Star} onclick={() => addFav(s)}>Add</Button>
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted small">This repository lists no game servers.</p>
        {/if}
      </Section>
      <Section title="Share">
        <CopyField label="Repository URL" value={repo.url} hint="Give this to other players, or the auto-config link from the community's page." />
      </Section>
    </div>
  </div>
</div>

<style>
  .cols { display: grid; grid-template-columns: minmax(0, 3fr) minmax(0, 2fr); gap: 16px; align-items: start; }
  @media (max-width: 1050px) { .cols { grid-template-columns: 1fr; } }
  .chips { max-height: 320px; overflow: auto; }
  .chip.mono { font-family: var(--mono); font-size: 12px; }
</style>
