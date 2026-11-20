<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import { Skeleton, EmptyState, downloadCsv } from "@axs/kit";
  import { Download, ChartNoAxesColumn } from "@lucide/svelte";

  let {
    title,
    description,
    icon: Icon,
    loading = false,
    empty = false,
    emptyText = "No data in this period.",
    csv,
    csvName = "export",
    height = 260,
    actions,
    children,
  }: {
    title: string;
    description?: string;
    icon?: Component<any>;
    loading?: boolean;
    empty?: boolean;
    emptyText?: string;
    /** Rows for the CSV export button. */
    csv?: () => Record<string, unknown>[];
    csvName?: string;
    height?: number;
    actions?: Snippet;
    children: Snippet;
  } = $props();
</script>

<section class="card cc">
  <header>
    <div class="t">
      <h2>{#if Icon}<Icon size={16} />{/if}{title}</h2>
      {#if description}<p class="muted small">{description}</p>{/if}
    </div>
    <div class="row tight">
      {@render actions?.()}
      {#if csv && !loading && !empty}
        <button class="btn ghost icon sm" data-tip="Export CSV" data-tip-pos="left" aria-label={`Export ${title} as CSV`} onclick={() => downloadCsv(csvName, csv())}><Download size={14} /></button>
      {/if}
    </div>
  </header>
  {#if loading}
    <div style:height="{height}px" class="sk"><Skeleton block height={height - 20} /></div>
  {:else if empty}
    <div style:min-height="{Math.min(height, 200)}px" class="emp"><EmptyState icon={ChartNoAxesColumn} title="Nothing yet" description={emptyText} compact /></div>
  {:else}
    {@render children()}
  {/if}
</section>

<style>
  .cc { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; }
  .t { min-width: 0; }
  h2 { margin: 0; display: flex; align-items: center; gap: 8px; font-size: 15.5px; }
  h2 :global(svg) { color: var(--accent-text); }
  p { margin: 3px 0 0; }
  .sk { padding-top: 10px; }
  .emp { display: grid; place-items: center; }
</style>
