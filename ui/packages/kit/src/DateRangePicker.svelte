<script lang="ts">
  import { CalendarRange } from "@lucide/svelte";
  import type { DateRange as Range } from "./types";

  let { value = $bindable(), onchange }: { value: Range; onchange?: (r: Range) => void } = $props();

  const presets: [string, string, number][] = [
    ["24h", "24 h", 1],
    ["7d", "7 days", 7],
    ["30d", "30 days", 30],
    ["90d", "90 days", 90],
    ["1y", "1 year", 365],
  ];
  let custom = $state(false);
  let fromDay = $state("");
  let toDay = $state("");

  function pick(id: string, days: number) {
    const to = Date.now();
    value = { from: to - days * 86_400_000, to, preset: id };
    custom = false;
    onchange?.(value);
  }
  function openCustom() {
    custom = !custom;
    const iso = (t: number) => new Date(t - new Date(t).getTimezoneOffset() * 60000).toISOString().slice(0, 10);
    fromDay = iso(value.from);
    toDay = iso(value.to);
  }
  function applyCustom() {
    const f = new Date(fromDay + "T00:00:00").getTime();
    const t = new Date(toDay + "T23:59:59").getTime();
    if (!isFinite(f) || !isFinite(t) || t <= f) return;
    value = { from: f, to: Math.min(t, Date.now()), preset: "custom" };
    custom = false;
    onchange?.(value);
  }
</script>

<div class="drp">
  <div class="seg" role="radiogroup" aria-label="Time range">
    {#each presets as [id, label, days]}
      <button type="button" role="radio" aria-checked={value.preset === id} class:on={value.preset === id} onclick={() => pick(id, days)}>{label}</button>
    {/each}
    <button type="button" class:on={value.preset === "custom"} onclick={openCustom} aria-expanded={custom}><CalendarRange size={14} />{value.preset === "custom" ? `${new Date(value.from).toLocaleDateString()} – ${new Date(value.to).toLocaleDateString()}` : "Custom"}</button>
  </div>
  {#if custom}
    <div class="pop card raised">
      <label class="field">From <input type="date" bind:value={fromDay} max={toDay} /></label>
      <label class="field">To <input type="date" bind:value={toDay} min={fromDay} /></label>
      <button class="btn primary sm" onclick={applyCustom}>Apply</button>
    </div>
  {/if}
</div>

<style>
  .drp { position: relative; }
  .seg { display: inline-flex; flex-wrap: wrap; padding: 2px; gap: 2px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r); }
  button {
    display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 11px; border: 0; border-radius: var(--r-sm);
    background: none; color: var(--fg-2); font: inherit; font-size: 13px; font-weight: 550; cursor: pointer; white-space: nowrap;
  }
  button:hover:not(.on) { color: var(--fg); background: var(--surface-3); }
  button.on { background: var(--surface-3); color: var(--fg); box-shadow: 0 0 0 1px var(--line-strong) inset; }
  .pop { position: absolute; right: 0; top: calc(100% + 6px); z-index: 30; display: flex; gap: 10px; align-items: flex-end; padding: 12px; box-shadow: var(--shadow-lg); }
  .pop input { width: 150px; }
</style>
