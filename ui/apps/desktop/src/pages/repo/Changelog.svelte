<script lang="ts">
  import { Button, Skeleton, EmptyState, date, ago } from "@axs/kit";
  import { History, CircleAlert, RotateCw } from "@lucide/svelte";
  import type { RepoInfo } from "../../api";

  let { info, loading, error, onretry }: { info: RepoInfo | null; loading: boolean; error: string | null; onretry: () => void } = $props();
  const entries = $derived([...(info?.changelogs ?? [])].sort((a, b) => b.revision - a.revision));
</script>

{#if error}
  <div class="card"><EmptyState icon={CircleAlert} title="Could not load the changelog" description={error} compact><Button icon={RotateCw} onclick={onretry}>Retry</Button></EmptyState></div>
{:else if loading && !info}
  <div class="card"><Skeleton lines={5} /></div>
{:else if !entries.length}
  <div class="card"><EmptyState icon={History} title="No changelog" compact /></div>
{:else}
  <ol class="tl">
    {#each entries as c (c.revision)}
      <li>
        <div class="marker"><span>{c.revision}</span></div>
        <div class="card body">
          <div class="row between">
            <span class="strong">Revision {c.revision}</span>
            <span class="small muted" title={date(c.build_date)}>{ago(c.build_date)}</span>
          </div>
          {#if c.new_addons.length}<div class="line"><span class="badge ok">Added</span><span class="mono small">{c.new_addons.join(", ")}</span></div>{/if}
          {#if c.updated_addons.length}<div class="line"><span class="badge info">Updated</span><span class="mono small">{c.updated_addons.join(", ")}</span></div>{/if}
          {#if c.deleted_addons.length}<div class="line"><span class="badge err">Removed</span><span class="mono small">{c.deleted_addons.join(", ")}</span></div>{/if}
          {#if !c.new_addons.length && !c.updated_addons.length && !c.deleted_addons.length}
            <span class="small muted">{c.content_updated ? "Content updated" : "Settings changed, no content change"}</span>
          {/if}
        </div>
      </li>
    {/each}
  </ol>
{/if}

<style>
  .tl { list-style: none; margin: 0; padding: 0; position: relative; display: flex; flex-direction: column; gap: 12px; }
  .tl::before { content: ""; position: absolute; left: 19px; top: 10px; bottom: 10px; width: 2px; background: var(--line); }
  .tl li { display: flex; gap: 14px; align-items: flex-start; position: relative; }
  .marker { width: 40px; flex-shrink: 0; display: grid; place-items: center; padding-top: 10px; }
  .marker span {
    min-width: 34px; height: 24px; padding: 0 6px; border-radius: var(--r-sm); display: grid; place-items: center;
    background: var(--surface-3); border: 1px solid var(--line-strong); font: 700 12px/1 var(--font-display); letter-spacing: 0.04em;
  }
  .tl li:first-child .marker span { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .body { flex: 1; display: flex; flex-direction: column; gap: 8px; padding: 12px 14px; min-width: 0; }
  .line { display: flex; gap: 10px; align-items: baseline; }
  .line .mono { overflow-wrap: anywhere; }
  .line .badge { flex-shrink: 0; width: 74px; justify-content: center; }
</style>
