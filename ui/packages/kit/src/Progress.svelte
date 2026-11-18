<script lang="ts">
  let { value = 0, max = 100, label = "" }: { value?: number; max?: number; label?: string } = $props();
  const pct = $derived(max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0);
</script>

<div class="wrap">
  {#if label}<div class="label small"><span>{label}</span><span class="muted">{pct.toFixed(0)}%</span></div>{/if}
  <div class="bar" role="progressbar" aria-valuenow={Math.round(pct)} aria-valuemin={0} aria-valuemax={100}><div style="width:{pct}%"></div></div>
</div>

<style>
  .wrap { display: flex; flex-direction: column; gap: 4px; }
  .label { display: flex; justify-content: space-between; }
  .bar { height: 8px; background: var(--panel-2); border: 1px solid var(--line); border-radius: 999px; overflow: hidden; }
  .bar > div { height: 100%; background: var(--accent); transition: width 0.2s; }
</style>
