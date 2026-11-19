<script lang="ts">
  import type { Component, Snippet } from "svelte";
  let {
    label,
    value,
    hint,
    tone,
    icon: Icon,
    children,
  }: { label: string; value: string | number; hint?: string; tone?: "ok" | "warn" | "err" | "accent"; icon?: Component<any>; children?: Snippet } = $props();
</script>

<div class="stat card">
  <div class="lbl">{#if Icon}<Icon size={14} />{/if}{label}</div>
  <div class="val {tone ?? ''}">{value}</div>
  {#if hint}<div class="hint">{hint}</div>{/if}
  {@render children?.()}
</div>

<style>
  .stat { display: flex; flex-direction: column; gap: 4px; padding: 14px 16px; min-width: 0; }
  .lbl { display: flex; align-items: center; gap: 6px; font-family: var(--font-display); font-size: 12.5px; font-weight: 600; letter-spacing: 0.1em; text-transform: uppercase; color: var(--fg-3); }
  .val { font-family: var(--font-display); font-size: 30px; font-weight: 600; line-height: 1.05; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
  .val.ok { color: var(--ok); }
  .val.warn { color: var(--warn); }
  .val.err { color: var(--err); }
  .val.accent { color: var(--accent-text); }
  .hint { font-size: 12.5px; color: var(--fg-2); }
</style>
