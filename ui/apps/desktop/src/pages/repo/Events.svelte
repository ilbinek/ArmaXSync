<script lang="ts">
  import { Button, Skeleton, EmptyState, toast, toastError } from "@axs/kit";
  import { CalendarDays, Download, Layers, CircleAlert, RotateCw } from "@lucide/svelte";
  import { call, type RepoConfig, type RepoInfo } from "../../api";
  import { app, loadProfile } from "../../store.svelte";

  let { repo, info, loading, error, onretry, ondownload }: { repo: RepoConfig; info: RepoInfo | null; loading: boolean; error: string | null; onretry: () => void; ondownload: (event: string) => void } = $props();

  async function addModset(ev: string) {
    try {
      await call("apply_modset", { profile: app.state!.active_profile, repo: repo.name, event: ev });
      await loadProfile();
      toast(`Event ${ev} added to profile ${app.state!.active_profile}`, "ok", { action: { label: "Open modsets", run: () => (location.hash = "#/modsets") } });
    } catch (e) {
      toastError(e);
    }
  }
</script>

{#if error}
  <div class="card"><EmptyState icon={CircleAlert} title="Could not load events" description={error} compact><Button icon={RotateCw} onclick={onretry}>Retry</Button></EmptyState></div>
{:else if loading && !info}
  <div class="card"><Skeleton lines={4} /></div>
{:else if !info?.events.length}
  <div class="card"><EmptyState icon={CalendarDays} title="No events" description="Events are named addon selections (an operation, a training night). This repository defines none." compact /></div>
{:else}
  <div class="evs">
    {#each info.events as e (e.name)}
      {@const req = e.addons.filter((a) => !a.optional)}
      {@const opt = e.addons.filter((a) => a.optional)}
      <article class="card ev">
        <header>
          <div class="grow">
            <h2>{e.name}</h2>
            {#if e.description}<p class="muted small">{e.description}</p>{/if}
          </div>
        </header>
        <div class="xs eyebrow">{req.length} required{opt.length ? ` · ${opt.length} optional` : ""}</div>
        <div class="chips">
          {#each req as a}<span class="chip mono">{a.name}</span>{/each}
          {#each opt as a}<span class="chip mono opt" title="Optional">{a.name}</span>{/each}
        </div>
        <footer class="row end">
          <Button size="sm" icon={Layers} onclick={() => addModset(e.name)}>Add to profile</Button>
          <Button size="sm" variant="primary" icon={Download} onclick={() => ondownload(e.name)}>Download</Button>
        </footer>
      </article>
    {/each}
  </div>
{/if}

<style>
  .evs { display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 14px; }
  .ev { display: flex; flex-direction: column; gap: 10px; }
  .ev h2 { margin: 0 0 2px; }
  .ev p { margin: 0; }
  .ev footer { margin-top: auto; padding-top: 6px; }
  .chip.mono { font-family: var(--mono); font-size: 12px; }
  .chip.opt { border-style: dashed; color: var(--fg-2); background: transparent; }
</style>
