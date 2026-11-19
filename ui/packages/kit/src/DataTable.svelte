<script lang="ts" generics="T">
  import type { Component, Snippet } from "svelte";
  import { ArrowDown, ArrowUp, Search, CircleAlert, RotateCw } from "@lucide/svelte";
  import Skeleton from "./Skeleton.svelte";
  import EmptyState from "./EmptyState.svelte";
  import type { Column } from "./types";

  let {
    rows,
    columns,
    cell,
    key,
    onrowclick,
    loading = false,
    error = null,
    onretry,
    search,
    searchPlaceholder = "Filter…",
    emptyTitle = "Nothing here yet",
    emptyText,
    emptyIcon,
    empty,
    toolbar,
    sortBy,
    sortDesc = false,
    rowClass,
    dense = false,
  }: {
    rows: T[] | null | undefined;
    columns: Column<T>[];
    cell: Snippet<[T, Column<T>]>;
    key: (r: T) => string | number;
    onrowclick?: (r: T) => void;
    loading?: boolean;
    error?: string | null;
    onretry?: () => void;
    /** Enables the filter box; return true to keep the row. */
    search?: (r: T, q: string) => boolean;
    searchPlaceholder?: string;
    emptyTitle?: string;
    emptyText?: string;
    emptyIcon?: Component<any>;
    empty?: Snippet;
    toolbar?: Snippet;
    sortBy?: string;
    sortDesc?: boolean;
    rowClass?: (r: T) => string;
    dense?: boolean;
  } = $props();

  let q = $state("");
  let sKey = $state<string | undefined>(undefined);
  let sDesc = $state(false);
  $effect.pre(() => {
    if (sKey === undefined && sortBy) {
      sKey = sortBy;
      sDesc = sortDesc;
    }
  });

  const shown = $derived.by(() => {
    let r = rows ?? [];
    const query = q.trim().toLowerCase();
    if (query && search) r = r.filter((x) => search(x, query));
    const col = columns.find((c) => c.key === sKey);
    if (col?.sort) {
      const f = col.sort;
      r = [...r].sort((a, b) => {
        const va = f(a);
        const vb = f(b);
        if (va == null && vb == null) return 0;
        if (va == null) return 1;
        if (vb == null) return -1;
        const c = typeof va === "number" && typeof vb === "number" ? va - vb : String(va).localeCompare(String(vb), undefined, { numeric: true, sensitivity: "base" });
        return sDesc ? -c : c;
      });
    }
    return r;
  });

  function sortOn(c: Column<T>) {
    if (!c.sort) return;
    if (sKey === c.key) sDesc = !sDesc;
    else {
      sKey = c.key;
      sDesc = false;
    }
  }
</script>

<div class="dt">
  {#if search || toolbar}
    <div class="tb">
      {#if search}
        <div class="search">
          <Search size={14} />
          <input type="search" bind:value={q} placeholder={searchPlaceholder} aria-label={searchPlaceholder} />
        </div>
      {/if}
      {@render toolbar?.()}
    </div>
  {/if}

  {#if error}
    <div class="state">
      <EmptyState icon={CircleAlert} title="Could not load" description={error} compact>
        {#if onretry}<button class="btn" onclick={onretry}><RotateCw size={14} />Retry</button>{/if}
      </EmptyState>
    </div>
  {:else if loading && !rows?.length}
    <div class="state pad"><Skeleton lines={4} height={18} /></div>
  {:else if !rows?.length}
    <div class="state">
      {#if empty}{@render empty()}{:else}<EmptyState icon={emptyIcon} title={emptyTitle} description={emptyText} compact />{/if}
    </div>
  {:else}
    <div class="table-wrap">
      <table class:dense>
        <thead>
          <tr>
            {#each columns as c (c.key)}
              <th
                class={c.class}
                class:hide-sm={c.hideSm}
                style:width={c.width}
                style:text-align={c.align}
                aria-sort={sKey === c.key ? (sDesc ? "descending" : "ascending") : undefined}
              >
                {#if c.sort}
                  <button class="sortbtn" class:on={sKey === c.key} onclick={() => sortOn(c)}>
                    {c.label}
                    {#if sKey === c.key}{#if sDesc}<ArrowDown size={12} />{:else}<ArrowUp size={12} />{/if}{/if}
                  </button>
                {:else}{c.label}{/if}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each shown as r (key(r))}
            <tr
              class:clickable={!!onrowclick}
              class={rowClass?.(r)}
              onclick={onrowclick ? (e) => {
                // Let buttons/links inside the row work on their own.
                if ((e.target as HTMLElement).closest("button, a, input, select, label")) return;
                onrowclick(r);
              } : undefined}
              onkeydown={onrowclick ? (e) => e.key === "Enter" && e.target === e.currentTarget && onrowclick(r) : undefined}
              tabindex={onrowclick ? 0 : undefined}
            >
              {#each columns as c (c.key)}
                <td class={c.class} class:hide-sm={c.hideSm} style:text-align={c.align}>{@render cell(r, c)}</td>
              {/each}
            </tr>
          {:else}
            <tr><td colspan={columns.length} class="nomatch">No matches for “{q}”.</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .dt { display: flex; flex-direction: column; min-width: 0; }
  .tb { display: flex; gap: var(--s2); align-items: center; padding: 10px 12px; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
  .search { position: relative; flex: 1; min-width: 180px; max-width: 340px; }
  .search :global(svg) { position: absolute; left: 9px; top: 50%; transform: translateY(-50%); color: var(--fg-3); }
  .search input { padding-left: 30px; height: 30px; }
  .state.pad { padding: var(--s4); }
  .sortbtn { display: inline-flex; align-items: center; gap: 4px; background: none; border: 0; padding: 0; font: inherit; color: inherit; text-transform: inherit; letter-spacing: inherit; cursor: pointer; }
  .sortbtn:hover, .sortbtn.on { color: var(--fg); }
  table.dense :global(td) { padding-top: 6px; padding-bottom: 6px; }
  tr:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .nomatch { text-align: center; color: var(--fg-2); padding: var(--s5); }
</style>
