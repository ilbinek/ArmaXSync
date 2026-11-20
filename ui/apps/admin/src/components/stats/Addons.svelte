<script lang="ts">
  import { DataTable, Sparkline, bytes } from "@axs/kit";
  import { Package } from "@lucide/svelte";
  import Chart from "../charts/Chart.svelte";
  import ChartCard from "../charts/ChartCard.svelte";
  import { fmt, palette, valueAxis, type EChartsOption } from "../charts/echarts";
  import { loader } from "./load.svelte";

  let { q, days, repo }: { q: string; days: number; repo: string } = $props();

  const ad = loader<any>(() => `/stats/addons?${q}&limit=100`);
  const rows = $derived((ad.data?.addons ?? []) as any[]);
  const total = $derived(rows.reduce((s, r) => s + r.bytes, 0));
  const multi = $derived(!repo && new Set(rows.map((r) => r.repo)).size > 1);

  const topOpt = $derived.by((): EChartsOption => {
    const top = rows.slice(0, 15).reverse();
    const accent = palette()[0];
    return {
      grid: { left: 8, right: 70, top: 8, bottom: 8, containLabel: true },
      tooltip: {
        trigger: "axis",
        axisPointer: { type: "shadow" },
        formatter: (ps: any) => {
          const r = top[ps[0].dataIndex];
          return `<b>${r.addon}</b>${multi ? ` <span style="opacity:.7">${r.repo}</span>` : ""}<br/>${bytes(r.bytes)} · ${r.downloads.toLocaleString()} full downloads`;
        },
      },
      xAxis: valueAxis(fmt.bytes),
      yAxis: { type: "category", data: top.map((r) => (multi ? `${r.addon} · ${r.repo}` : r.addon)), splitLine: { show: false }, axisLabel: { fontFamily: "var(--mono)" } },
      series: [{ type: "bar", data: top.map((r) => r.bytes), itemStyle: { color: accent, borderRadius: [0, 3, 3, 0] }, barMaxWidth: 18, label: { show: true, position: "right", color: "inherit", fontSize: 11, formatter: (p: any) => bytes(p.value) } }],
    };
  });

  const columns = $derived([
    { key: "addon", label: "Addon", sort: (r: any) => r.addon.toLowerCase() },
    ...(multi ? [{ key: "repo", label: "Repository", sort: (r: any) => r.repo, hideSm: true }] : []),
    { key: "bytes", label: "Served", sort: (r: any) => r.bytes, align: "right" as const },
    { key: "share", label: "Share", sort: (r: any) => r.bytes, align: "right" as const, hideSm: true },
    { key: "downloads", label: "Full downloads", sort: (r: any) => r.downloads, align: "right" as const },
    { key: "trend", label: `Trend (${days} d)`, hideSm: true },
  ]);
</script>

<ChartCard title="Most downloaded addons" icon={Package} description="By data served, including partial updates" loading={ad.loading && !ad.data} empty={!!ad.data && !rows.length} emptyText="No addon downloads in this period." csv={() => rows.map(({ trend, ...r }) => r)} csvName="addons" height={Math.max(220, Math.min(15, rows.length) * 30)}>
  <Chart option={topOpt} height={Math.max(220, Math.min(15, rows.length) * 30)} label="Top addons" />
</ChartCard>

<section class="card flush">
  <DataTable rows={rows} {columns} key={(r) => `${r.repo}/${r.addon}`} loading={ad.loading && !ad.data} error={ad.error} sortBy="bytes" sortDesc search={(r, s) => r.addon.toLowerCase().includes(s)} searchPlaceholder="Filter addons…" emptyIcon={Package} emptyTitle="No addon downloads">
    {#snippet cell(r, c)}
      {#if c.key === "addon"}<span class="mono small">{r.addon}</span>
      {:else if c.key === "repo"}<span class="small muted">{r.repo}</span>
      {:else if c.key === "bytes"}<span class="tabular">{bytes(r.bytes)}</span>
      {:else if c.key === "share"}
        <span class="share"><span class="bar" style:width="{total ? (r.bytes / total) * 100 : 0}%"></span><span class="tabular xs">{total ? ((r.bytes / total) * 100).toFixed(1) : 0}%</span></span>
      {:else if c.key === "downloads"}<span class="tabular">{r.downloads.toLocaleString()}</span>
      {:else if c.key === "trend"}
        {#if r.trend}<Sparkline values={r.trend} width={110} height={24} />{:else}<span class="subtle xs">top 10 only</span>{/if}
      {/if}
    {/snippet}
  </DataTable>
</section>

<style>
  .share { display: inline-flex; align-items: center; gap: 8px; justify-content: flex-end; width: 120px; }
  .bar { height: 6px; border-radius: 3px; background: var(--accent); opacity: 0.75; max-width: 70px; min-width: 2px; }
</style>
