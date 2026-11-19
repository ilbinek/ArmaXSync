<script lang="ts">
  // Minimal bar chart: one series, labelled y-axis, hover readout.
  let {
    data,
    height = 180,
    format = (v: number) => String(v),
    label = "",
    compact = false,
  }: {
    data: { label: string; value: number }[];
    height?: number;
    format?: (v: number) => string;
    label?: string;
    /** Sparkline style: no axis, no labels. */
    compact?: boolean;
  } = $props();

  let hover = $state<number | null>(null);
  const max = $derived(Math.max(1, ...data.map((d) => d.value)));
  // A "nice" axis maximum.
  const top = $derived.by(() => {
    const p = Math.pow(10, Math.floor(Math.log10(max)));
    const n = [1, 2, 2.5, 5, 10].find((m) => m * p >= max) ?? 10;
    return n * p;
  });
  const W = 600;
  const padL = $derived(compact ? 0 : 64);
  const padB = $derived(compact ? 0 : 22);
  const H = $derived(height);
  const bw = $derived(data.length ? (W - padL) / data.length : 0);
  const y = (v: number) => (H - padB) * (1 - v / top);
  const ticks = $derived(compact ? [] : [0, 0.25, 0.5, 0.75, 1].map((f) => f * top));
  const shown = $derived(hover != null ? data[hover] : null);
</script>

<div class="chart" class:compact>
  {#if !compact}
    <div class="readout small">
      {#if shown}<b>{format(shown.value)}</b> <span class="muted">{shown.label}</span>{:else}<span class="muted">{label}</span>{/if}
    </div>
  {/if}
  <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" style:height="{H}px" role="img" aria-label={label} onmouseleave={() => (hover = null)}>
    {#each ticks as t}
      <line x1={padL} x2={W} y1={y(t)} y2={y(t)} class="grid" />
      <text x={padL - 8} y={y(t) + 4} class="tick" text-anchor="end">{format(t)}</text>
    {/each}
    {#each data as d, i}
      {@const h = (H - padB) - y(d.value)}
      <rect
        x={padL + i * bw + bw * 0.15}
        y={y(d.value)}
        width={Math.max(1, bw * 0.7)}
        height={Math.max(d.value > 0 ? 1.5 : 0, h)}
        class="bar"
        class:on={hover === i}
        role="presentation"
        onmouseenter={() => (hover = i)}
      />
      <rect x={padL + i * bw} y="0" width={bw} height={H - padB} fill="transparent" role="presentation" onmouseenter={() => (hover = i)} />
    {/each}
    {#if !compact && data.length}
      <text x={padL} y={H - 5} class="tick">{data[0].label}</text>
      <text x={W} y={H - 5} class="tick" text-anchor="end">{data[data.length - 1].label}</text>
    {/if}
  </svg>
</div>

<style>
  .chart { display: flex; flex-direction: column; gap: 6px; }
  .readout { min-height: 20px; }
  svg { width: 100%; display: block; overflow: visible; }
  .grid { stroke: var(--line); stroke-width: 1; vector-effect: non-scaling-stroke; }
  .tick { fill: var(--fg-3); font-size: 11px; font-family: var(--font); }
  .bar { fill: var(--accent); opacity: 0.75; transition: opacity var(--dur); }
  .bar.on { opacity: 1; }
  .compact .bar { opacity: 0.85; }
</style>
