<script lang="ts">
  import { SegmentedControl, Stat, bytes } from "@axs/kit";
  import { Activity, ArrowDownUp, CircleCheck, Gauge, TriangleAlert, Grid3x3 } from "@lucide/svelte";
  import Chart from "../charts/Chart.svelte";
  import ChartCard from "../charts/ChartCard.svelte";
  import { area, axisTooltip, fmt, valueAxis, dayLabel, hourLabel, palette, hexA, type EChartsOption } from "../charts/echarts";
  import { loader } from "./load.svelte";

  let { q, days, repo }: { q: string; days: number; repo: string } = $props();

  let bucket = $state<"auto" | "hour" | "day">("auto");
  let heatMetric = $state<"served" | "requests" | "downloads">("served");

  const tr = loader<any>(() => `/stats/traffic?${q}${bucket === "auto" ? "" : `&bucket=${bucket}`}`);
  const hm = loader<any>(() => `/stats/heatmap?${q}`);

  const s = $derived((tr.data?.series ?? []) as any[]);
  const hourly = $derived(tr.data?.bucket === "hour");
  const x = $derived(s.map((r) => (hourly ? hourLabel(r.t) : dayLabel(new Date(r.t).toISOString().slice(0, 10)))));
  const t = $derived(tr.data?.total);

  const byRepoOpt = $derived.by((): EChartsOption => {
    const rows = (tr.data?.by_repo ?? []) as any[];
    const repos = [...new Set(rows.map((r) => r.repo))].filter(Boolean) as string[];
    const idx = new Map(s.map((r, i) => [r.t, i]));
    const pal = palette();
    return {
      tooltip: axisTooltip(fmt.bytes),
      legend: repos.length > 1 ? { data: repos, type: "scroll" } : undefined,
      xAxis: { type: "category", data: x },
      yAxis: valueAxis(fmt.bytes),
      dataZoom: s.length > 60 ? [{ type: "inside" }] : undefined,
      series: repos.map((name, i) => {
        const data = new Array(s.length).fill(0);
        for (const r of rows) if (r.repo === name && idx.has(r.t)) data[idx.get(r.t)!] += r.served;
        return { type: "bar", name, stack: "s", data, barMaxWidth: 26, itemStyle: { color: pal[i % pal.length], borderRadius: i === repos.length - 1 ? [3, 3, 0, 0] : 0 } };
      }),
    };
  });

  const requestsOpt = $derived.by((): EChartsOption => {
    const [a, b, c, d] = palette();
    return {
      tooltip: axisTooltip(fmt.num),
      legend: { data: ["Requests", "Update checks", "Full downloads", "Partial"] },
      xAxis: { type: "category", data: x, boundaryGap: false },
      yAxis: valueAxis(fmt.num),
      series: [
        { ...area(a, 0.18), name: "Requests", data: s.map((r) => r.requests) },
        { type: "line", name: "Update checks", smooth: 0.25, showSymbol: false, data: s.map((r) => r.checks), itemStyle: { color: b } },
        { type: "line", name: "Full downloads", smooth: 0.25, showSymbol: false, data: s.map((r) => r.files), itemStyle: { color: c } },
        { type: "line", name: "Partial", smooth: 0.25, showSymbol: false, data: s.map((r) => r.ranges), itemStyle: { color: d } },
      ],
    };
  });

  const outcomeOpt = $derived.by((): EChartsOption => {
    const pal = palette();
    return {
      tooltip: axisTooltip(fmt.num),
      legend: { data: ["Completed", "Aborted"] },
      xAxis: { type: "category", data: x },
      yAxis: valueAxis(fmt.num),
      series: [
        { type: "bar", name: "Completed", stack: "o", data: s.map((r) => r.completed), itemStyle: { color: pal[2] }, barMaxWidth: 24 },
        { type: "bar", name: "Aborted", stack: "o", data: s.map((r) => r.aborted), itemStyle: { color: pal[5], borderRadius: [3, 3, 0, 0] }, barMaxWidth: 24 },
      ],
    };
  });

  const speedOpt = $derived.by((): EChartsOption => {
    const pal = palette();
    return {
      tooltip: axisTooltip(fmt.speed),
      xAxis: { type: "category", data: x, boundaryGap: false },
      yAxis: valueAxis(fmt.speed),
      series: [{ ...area(pal[1], 0.2), name: "Average download speed", connectNulls: true, data: s.map((r) => (r.xfer_ms > 0 ? r.avg_speed : null)) }],
    };
  });

  const statusOpt = $derived.by((): EChartsOption => {
    const pal = palette();
    const groups: [string, (r: any) => number, string][] = [
      ["404 not found", (r) => r.s404, pal[5]],
      ["401 login", (r) => r.s401, pal[3]],
      ["503 busy", (r) => r.s503, pal[6]],
      ["Other 4xx", (r) => r.s4xx + r.s416, pal[7]],
      ["5xx", (r) => r.s5xx, "#ef4444"],
    ];
    return {
      tooltip: axisTooltip(fmt.num),
      legend: { data: groups.map((g) => g[0]) },
      xAxis: { type: "category", data: x },
      yAxis: valueAxis(fmt.num, { minInterval: 1 }),
      series: groups.map(([name, f, color]) => ({ type: "bar", name, stack: "e", data: s.map(f), itemStyle: { color }, barMaxWidth: 24 })),
    };
  });

  const heatOpt = $derived.by((): EChartsOption => {
    const cells = (hm.data?.cells ?? []) as any[];
    const vals = cells.map((c) => c[heatMetric]);
    const max = Math.max(1, ...vals);
    const wd = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const f = heatMetric === "served" ? fmt.bytes : fmt.num;
    const accent = palette()[0];
    return {
      grid: { left: 8, right: 12, top: 8, bottom: 40, containLabel: true },
      tooltip: { trigger: "item", formatter: (p: any) => `${wd[p.value[1]]} ${String(p.value[0]).padStart(2, "0")}:00–${String(p.value[0] + 1).padStart(2, "0")}:00<br/><b>${f(p.value[2])}</b>` },
      xAxis: { type: "category", data: Array.from({ length: 24 }, (_, h) => String(h).padStart(2, "0")), splitArea: { show: false }, axisLabel: { interval: 1 } },
      yAxis: { type: "category", data: wd, inverse: true, splitLine: { show: false } },
      visualMap: { min: 0, max, show: true, orient: "horizontal", left: "center", bottom: 0, itemHeight: 140, itemWidth: 10, calculable: false, inRange: { color: [hexA(accent, 0.04), hexA(accent, 0.35), accent] }, textStyle: { color: "inherit" }, formatter: (v: any) => f(Number(v)) },
      series: [{ type: "heatmap", data: cells.map((c) => [c.hour, c.weekday, c[heatMetric]]), itemStyle: { borderRadius: 3, borderWidth: 2, borderColor: "transparent" }, emphasis: { itemStyle: { borderColor: accent } } }],
    };
  });

  const csvSeries = () => s.map((r) => ({ time: new Date(r.t).toISOString(), ...Object.fromEntries(Object.entries(r).filter(([k]) => k !== "t")) }));
</script>

<div class="row between">
  <span class="small muted">{hourly ? "Hourly" : "Daily"} buckets in your local time · {days} day{days > 1 ? "s" : ""}{repo ? ` · ${repo}` : ""}</span>
  <SegmentedControl size="sm" bind:value={bucket} options={[{ value: "auto", label: "Auto" }, { value: "hour", label: "Hourly" }, { value: "day", label: "Daily" }]} />
</div>

{#if t}
  <div class="grid4">
    <Stat label="Served" value={t.served} format={bytes} hint={`${bytes(t.bytes)} content · ${bytes(t.bytes_sidecar)} delta files · ${bytes(t.bytes_meta)} metadata`} />
    <Stat label="Requests" value={t.requests} hint={`${t.heads.toLocaleString()} HEAD · ${t.ranges.toLocaleString()} partial`} />
    <Stat label="Avg download speed" value={t.avg_speed} format={fmt.speed} hint="per transfer, after limits" />
    <Stat label="Waited for a slot" value={t.wait_ms} format={fmt.ms} hint={t.s503 ? `${t.s503} rejected as busy` : "total queueing time"} tone={t.s503 ? "warn" : undefined} />
  </div>
{/if}

<ChartCard title="Data served" icon={Activity} description={repo ? "Per bucket" : "Per bucket and repository"} loading={tr.loading && !tr.data} empty={!!t && !t.served} csv={csvSeries} csvName="traffic" height={300}>
  <Chart option={byRepoOpt} height={300} label="Data served" />
</ChartCard>

<div class="cols">
  <ChartCard title="Requests" icon={ArrowDownUp} description="All requests, update checks and downloads" loading={tr.loading && !tr.data} empty={!!t && !t.requests} csv={csvSeries} csvName="requests">
    <Chart option={requestsOpt} label="Requests" />
  </ChartCard>
  <ChartCard title="Transfers" icon={CircleCheck} description="Downloads that finished vs. ones the client dropped" loading={tr.loading && !tr.data} empty={!!t && !(t.completed + t.aborted)} csv={csvSeries} csvName="transfers">
    <Chart option={outcomeOpt} label="Transfers" />
  </ChartCard>
  <ChartCard title="Download speed" icon={Gauge} description="Average per transfer" loading={tr.loading && !tr.data} empty={!!t && !t.xfer_ms} csv={csvSeries} csvName="speed">
    <Chart option={speedOpt} label="Download speed" />
  </ChartCard>
  <ChartCard title="Errors" icon={TriangleAlert} description="Failed requests by kind" loading={tr.loading && !tr.data} empty={!!t && !(t.s404 + t.s401 + t.s4xx + t.s416 + t.s503 + t.s5xx)} emptyText="No failed requests in this period." csv={csvSeries} csvName="errors">
    <Chart option={statusOpt} label="Errors" />
  </ChartCard>
</div>

<ChartCard title="Peak hours" icon={Grid3x3} description="When players download, by weekday and hour (your local time)" loading={hm.loading && !hm.data} empty={!!hm.data && !hm.data.cells.some((c: any) => c.requests)} csv={() => hm.data.cells} csvName="peak-hours" height={300}>
  {#snippet actions()}
    <SegmentedControl size="sm" bind:value={heatMetric} options={[{ value: "served", label: "Data" }, { value: "requests", label: "Requests" }, { value: "downloads", label: "Downloads" }]} />
  {/snippet}
  <Chart option={heatOpt} height={300} label="Peak hours heatmap" />
  {#if hm.data && hm.data.from > Date.now() - days * 86_400_000 + 3_600_000}
    <p class="xs subtle">Hourly detail is kept for a limited time; older days are not included.</p>
  {/if}
</ChartCard>

<style>
  .grid4 { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; }
  .grid4 :global(.val) { font-size: 24px; }
  .cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); gap: 16px; }
  @media (max-width: 520px) { .cols { grid-template-columns: 1fr; } }
</style>
