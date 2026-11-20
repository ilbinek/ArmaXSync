<script lang="ts">
  // Tiny inline trend line (no axes). Values are drawn left to right.
  let {
    values,
    width = 96,
    height = 28,
    tone = "accent",
    area = true,
    label = "",
  }: { values: number[]; width?: number; height?: number; tone?: "accent" | "ok" | "info" | "warn" | "err" | "muted"; area?: boolean; label?: string } = $props();

  const pts = $derived.by(() => {
    const v = values.length ? values : [0];
    const max = Math.max(...v, 1e-9);
    const min = Math.min(...v, 0);
    const span = max - min || 1;
    const step = v.length > 1 ? width / (v.length - 1) : width;
    return v.map((x, i) => [i * step, height - 2 - ((x - min) / span) * (height - 4)] as const);
  });
  const line = $derived(pts.map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join(" "));
  const fill = $derived(`${line} L${width},${height} L0,${height} Z`);
  const id = `sp-${Math.random().toString(36).slice(2, 8)}`;
</script>

<svg class="sp {tone}" {width} {height} viewBox="0 0 {width} {height}" role="img" aria-label={label || undefined} aria-hidden={label ? undefined : "true"}>
  <defs>
    <linearGradient {id} x1="0" x2="0" y1="0" y2="1">
      <stop offset="0" stop-color="currentColor" stop-opacity="0.32" />
      <stop offset="1" stop-color="currentColor" stop-opacity="0" />
    </linearGradient>
  </defs>
  {#if area}<path d={fill} fill="url(#{id})" />{/if}
  <path d={line} fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" class="ln" />
  {#if pts.length}<circle cx={pts[pts.length - 1][0]} cy={pts[pts.length - 1][1]} r="2.2" fill="currentColor" />{/if}
</svg>

<style>
  .sp { display: block; overflow: visible; color: var(--accent); }
  .sp.ok { color: var(--ok); }
  .sp.info { color: var(--info); }
  .sp.warn { color: var(--warn); }
  .sp.err { color: var(--err); }
  .sp.muted { color: var(--fg-3); }
  .ln { stroke-dasharray: 1000; stroke-dashoffset: 1000; animation: draw 900ms var(--ease) forwards; }
  @keyframes draw { to { stroke-dashoffset: 0; } }
</style>
