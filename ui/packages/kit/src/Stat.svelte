<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import { Tween } from "svelte/motion";
  import { cubicOut } from "svelte/easing";
  import { TrendingUp, TrendingDown, Minus } from "@lucide/svelte";
  import Sparkline from "./Sparkline.svelte";

  let {
    label,
    value,
    format,
    hint,
    tone,
    icon: Icon,
    spark,
    delta,
    /** A rise is bad (errors, aborts): colour the change accordingly. */
    invert = false,
    deltaLabel = "vs previous period",
    children,
  }: {
    label: string;
    value: string | number;
    /** Formats numeric values (and animates them counting up). */
    format?: (n: number) => string;
    hint?: string;
    tone?: "ok" | "warn" | "err" | "accent";
    icon?: Component<any>;
    spark?: number[];
    /** Relative change, e.g. 0.12 = +12 %. */
    delta?: number | null;
    invert?: boolean;
    deltaLabel?: string;
    children?: Snippet;
  } = $props();

  const reduce = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const tween = new Tween(0, { duration: reduce ? 0 : 700, easing: cubicOut });
  $effect(() => {
    if (typeof value === "number") tween.target = value;
  });
  const shown = $derived(typeof value === "number" ? (format ? format(tween.current) : Math.round(tween.current).toLocaleString()) : value);
  const dTone = $derived(delta == null || Math.abs(delta) < 0.005 ? "flat" : (delta > 0) !== invert ? "up" : "down");
</script>

<div class="stat card">
  <div class="lbl">{#if Icon}<Icon size={14} />{/if}{label}</div>
  <div class="row nowrap between bottom">
    <div class="val {tone ?? ''}">{shown}</div>
    {#if spark && spark.length > 1}<Sparkline values={spark} tone={tone === "err" ? "err" : tone === "warn" ? "warn" : "accent"} />{/if}
  </div>
  {#if delta != null && isFinite(delta)}
    <div class="delta {dTone}" title={deltaLabel}>
      {#if dTone === "flat"}<Minus size={13} />{:else if delta > 0}<TrendingUp size={13} />{:else}<TrendingDown size={13} />{/if}
      {delta > 0 ? "+" : ""}{(delta * 100).toFixed(Math.abs(delta) < 0.1 ? 1 : 0)}%
      <span class="subtle">{deltaLabel}</span>
    </div>
  {:else if hint}
    <div class="hint">{hint}</div>
  {/if}
  {@render children?.()}
</div>

<style>
  .stat { display: flex; flex-direction: column; gap: 4px; padding: 14px 16px; min-width: 0; transition: border-color var(--dur) var(--ease), transform var(--dur) var(--ease); }
  .stat:hover { border-color: var(--line-strong); transform: translateY(-1px); }
  .bottom { align-items: flex-end; gap: 10px; }
  .lbl { display: flex; align-items: center; gap: 6px; font-family: var(--font-display); font-size: 12.5px; font-weight: 600; letter-spacing: 0.1em; text-transform: uppercase; color: var(--fg-3); }
  .val { font-family: var(--font-display); font-size: 30px; font-weight: 600; line-height: 1.05; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; min-width: 0; }
  .val.ok { color: var(--ok); }
  .val.warn { color: var(--warn); }
  .val.err { color: var(--err); }
  .val.accent { color: var(--accent-text); }
  .hint { font-size: 12.5px; color: var(--fg-2); }
  .delta { display: flex; align-items: center; gap: 4px; font-size: 12.5px; font-weight: 600; }
  .delta.up { color: var(--ok); }
  .delta.down { color: var(--err); }
  .delta.flat { color: var(--fg-3); }
  .delta .subtle { font-weight: 400; margin-left: 2px; }
</style>
