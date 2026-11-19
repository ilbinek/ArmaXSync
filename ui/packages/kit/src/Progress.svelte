<script lang="ts">
  let {
    value = 0,
    max = 100,
    label = "",
    detail = "",
    indeterminate = false,
    tone = "accent",
    size = "md",
    showPct = true,
  }: {
    value?: number;
    max?: number;
    label?: string;
    /** Second line under the bar (current file, ETA …). */
    detail?: string;
    indeterminate?: boolean;
    tone?: "accent" | "ok" | "warn" | "err" | "info";
    size?: "sm" | "md";
    showPct?: boolean;
  } = $props();
  const pct = $derived(max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0);
</script>

<div class="wrap">
  {#if label}
    <div class="label small"><span class="ellipsis">{label}</span>{#if showPct && !indeterminate}<span class="pct tabular">{pct.toFixed(0)}%</span>{/if}</div>
  {/if}
  <div
    class="bar {size} {tone}"
    class:ind={indeterminate}
    role="progressbar"
    aria-valuenow={indeterminate ? undefined : Math.round(pct)}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-label={label || undefined}
  >
    <div style:width={indeterminate ? undefined : `${pct}%`}></div>
  </div>
  {#if detail}<div class="detail xs ellipsis">{detail}</div>{/if}
</div>

<style>
  .wrap { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .label { display: flex; justify-content: space-between; gap: 12px; }
  .pct { color: var(--fg-2); font-weight: 600; }
  .bar { height: 8px; background: var(--surface-3); border-radius: 2px; overflow: hidden; position: relative; }
  .bar.sm { height: 4px; }
  .bar > div {
    height: 100%; border-radius: 2px; transition: width 0.25s var(--ease);
    background: var(--accent);
    background-image: repeating-linear-gradient(-45deg, transparent 0 6px, rgb(255 255 255 / 0.12) 6px 12px);
  }
  .bar.ok > div { background-color: var(--ok); }
  .bar.warn > div { background-color: var(--warn); }
  .bar.err > div { background-color: var(--err); }
  .bar.info > div { background-color: var(--info); }
  .bar.ind > div { position: absolute; width: 35%; animation: ind 1.2s var(--ease) infinite; }
  @keyframes ind { from { left: -35%; } to { left: 100%; } }
  .detail { color: var(--fg-3); font-family: var(--mono); }
</style>
