<script lang="ts">
  import { Search, CornerDownLeft } from "@lucide/svelte";
  import type { PaletteItem } from "./types";
  import { trapFocus } from "./focus";

  let {
    open = $bindable(false),
    items,
    placeholder = "Search pages, repositories and actions…",
  }: { open?: boolean; items: PaletteItem[]; placeholder?: string } = $props();

  let q = $state("");
  let sel = $state(0);
  let list = $state<HTMLElement>();

  $effect(() => {
    if (open) {
      q = "";
      sel = 0;
    }
  });

  /** Higher is better; -1 = no match. Substrings beat scattered letters. */
  function score(it: PaletteItem, query: string): number {
    if (!query) return 1;
    const hay = `${it.label} ${it.keywords ?? ""} ${it.group}`.toLowerCase();
    const label = it.label.toLowerCase();
    if (label.startsWith(query)) return 100 - label.length / 100;
    const i = hay.indexOf(query);
    if (i >= 0) return 80 - i / 10;
    // Subsequence match.
    let pos = 0;
    let gaps = 0;
    for (const ch of query) {
      const n = hay.indexOf(ch, pos);
      if (n < 0) return -1;
      gaps += n - pos;
      pos = n + 1;
    }
    return 40 - gaps / 5;
  }

  const results = $derived.by(() => {
    const query = q.trim().toLowerCase();
    return items
      .map((it) => ({ it, s: score(it, query) }))
      .map((r) => ({ ...r, s: r.s < 0 ? r.s : r.s + (r.it.weight ?? 0) }))
      .filter((r) => r.s >= 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 40)
      .map((r) => r.it);
  });
  const groups = $derived.by(() => {
    const m = new Map<string, PaletteItem[]>();
    for (const r of results) m.set(r.group, [...(m.get(r.group) ?? []), r]);
    return [...m.entries()];
  });

  function run(it: PaletteItem) {
    open = false;
    it.run();
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      open = false;
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      sel = Math.min(results.length - 1, sel + 1);
      scroll();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      sel = Math.max(0, sel - 1);
      scroll();
    } else if (e.key === "Enter" && results[sel]) {
      e.preventDefault();
      run(results[sel]);
    }
  }
  function scroll() {
    queueMicrotask(() => list?.querySelector(".on")?.scrollIntoView({ block: "nearest" }));
  }
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={() => (open = false)}></div>
  <div class="pal" role="dialog" aria-modal="true" aria-label="Command palette" tabindex="-1" use:trapFocus onkeydown={key}>
    <div class="inp">
      <Search size={17} />
      <input bind:value={q} oninput={() => (sel = 0)} {placeholder} aria-label="Search" data-autofocus spellcheck="false" />
      <kbd>Esc</kbd>
    </div>
    <div class="res" bind:this={list} role="listbox">
      {#each groups as [g, its]}
        <div class="g">{g}</div>
        {#each its as it}
          {@const i = results.indexOf(it)}
          <button role="option" aria-selected={i === sel} class:on={i === sel} onmouseenter={() => (sel = i)} onclick={() => run(it)}>
            {#if it.icon}<it.icon size={16} />{:else}<span class="noic"></span>{/if}
            <span class="grow ellipsis">{it.label}</span>
            {#if it.hint}<span class="hint">{it.hint}</span>{/if}
            {#if i === sel}<CornerDownLeft size={14} class="enter" />{/if}
          </button>
        {/each}
      {:else}
        <div class="none">No results for “{q}”.</div>
      {/each}
    </div>
    <div class="foot"><kbd>↑</kbd><kbd>↓</kbd> navigate <kbd>Enter</kbd> open <span class="grow"></span><kbd>Ctrl</kbd><kbd>K</kbd></div>
  </div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; z-index: 90; background: var(--overlay); backdrop-filter: blur(2px); animation: fade 120ms var(--ease); }
  .pal {
    position: fixed; z-index: 91; left: 50%; top: 12vh; transform: translateX(-50%); width: min(620px, calc(100vw - 24px));
    display: flex; flex-direction: column; max-height: 70vh; background: var(--surface-1); border: 1px solid var(--line-strong);
    border-top: 2px solid var(--accent); border-radius: var(--r-lg); box-shadow: var(--shadow-lg); outline: none; animation: pop 140ms var(--ease);
  }
  .inp { display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-bottom: 1px solid var(--line); }
  .inp :global(svg) { color: var(--accent-text); }
  .inp input { border: 0 !important; box-shadow: none !important; background: transparent; height: 30px; font-size: 15px; padding: 0; }
  .res { overflow: auto; padding: 6px; }
  .g { font-family: var(--font-display); font-size: 11.5px; font-weight: 600; letter-spacing: 0.12em; text-transform: uppercase; color: var(--fg-3); padding: 8px 10px 4px; }
  button { display: flex; align-items: center; gap: 10px; width: 100%; padding: 8px 10px; border: 0; border-radius: var(--r-sm); background: none; color: var(--fg); font: inherit; font-size: 14px; text-align: left; cursor: pointer; }
  button :global(svg) { color: var(--fg-2); flex-shrink: 0; }
  button.on { background: var(--accent-soft); }
  button.on :global(svg) { color: var(--accent-text); }
  .noic { width: 16px; }
  .hint { color: var(--fg-3); font-size: 12px; white-space: nowrap; }
  .none { padding: 18px; text-align: center; color: var(--fg-2); }
  .foot { display: flex; align-items: center; gap: 6px; padding: 8px 14px; border-top: 1px solid var(--line); font-size: 12px; color: var(--fg-3); }
  @keyframes fade { from { opacity: 0; } }
  @keyframes pop { from { opacity: 0; transform: translate(-50%, -6px); } }
</style>
