<script lang="ts">
  import { onMount } from "svelte";
  import { Stat, Alert, bytes, downloadCsv } from "@axs/kit";
  import { HardDriveDownload, Users, Download, RefreshCw, Radio, Gauge, Timer } from "@lucide/svelte";
  import { get } from "../../api";
  import Chart from "../charts/Chart.svelte";
  import ChartCard from "../charts/ChartCard.svelte";
  import { area, axisTooltip, fmt, valueAxis, dayLabel, hourLabel, palette, type EChartsOption } from "../charts/echarts";
  import { loader, change } from "./load.svelte";

  let { q, days, repo, onplayers }: { q: string; days: number; repo: string; onplayers?: (enabled: boolean) => void } = $props();

  const sum = loader<any>(() => `/stats/summary?${q}`);
  const tr = loader<any>(() => `/stats/traffic?${q}`);
  const pl = loader<any>(() => `/stats/players?${q}`);

  $effect(() => {
    if (sum.data) onplayers?.(sum.data.players_enabled);
  });

  // Live panel, polled every 3 s while visible.
  let live = $state<any>(null);
  onMount(() => {
    const tick = () => !document.hidden && get("/stats/live").then((l) => (live = l)).catch(() => {});
    tick();
    const t = setInterval(tick, 3000);
    return () => clearInterval(t);
  });

  const c = $derived(sum.data?.current);
  const p = $derived(sum.data?.previous);
  const daily = $derived((sum.data?.daily ?? []) as any[]);

  const trafficOpt = $derived.by((): EChartsOption => {
    const s = (tr.data?.series ?? []) as any[];
    const hourly = tr.data?.bucket === "hour";
    const x = s.map((r) => (hourly ? hourLabel(r.t) : dayLabel(new Date(r.t).toISOString().slice(0, 10))));
    const [a, b, cc] = palette();
    return {
      tooltip: axisTooltip(fmt.bytes),
      legend: { data: ["Content", "Delta files", "Metadata"] },
      xAxis: { type: "category", data: x, boundaryGap: false },
      yAxis: valueAxis(fmt.bytes),
      series: [
        { ...area(a, 0.35, true), name: "Content", stack: "b", data: s.map((r) => r.bytes) },
        { ...area(b, 0.35, true), name: "Delta files", stack: "b", data: s.map((r) => r.bytes_sidecar) },
        { ...area(cc, 0.35, true), name: "Metadata", stack: "b", data: s.map((r) => r.bytes_meta) },
      ],
    };
  });

  const playersOpt = $derived.by((): EChartsOption => {
    const d = (pl.data?.daily ?? []) as any[];
    const [a, b, cc] = palette();
    return {
      tooltip: axisTooltip(fmt.num),
      legend: { data: ["Daily", "Weekly", "Monthly"] },
      xAxis: { type: "category", data: d.map((r) => dayLabel(r.day)), boundaryGap: false },
      yAxis: valueAxis(fmt.num, { minInterval: 1 }),
      series: [
        { ...area(a, 0.28), name: "Daily", data: d.map((r) => r.players) },
        { type: "line", name: "Weekly", smooth: 0.25, showSymbol: false, lineStyle: { width: 1.6, color: b }, itemStyle: { color: b }, data: d.map((r) => r.weekly) },
        { type: "line", name: "Monthly", smooth: 0.25, showSymbol: false, lineStyle: { width: 1.6, type: "dashed", color: cc }, itemStyle: { color: cc }, data: d.map((r) => r.monthly) },
      ],
    };
  });

  const liveOpt = $derived.by((): EChartsOption => {
    const s = (live?.series ?? []) as [number, number][];
    return {
      grid: { left: 4, right: 4, top: 6, bottom: 4, containLabel: true },
      tooltip: axisTooltip(fmt.speed, (p) => new Date(p.value[0]).toLocaleTimeString()),
      xAxis: { type: "time", axisLabel: { show: false }, splitLine: { show: false } },
      yAxis: valueAxis(fmt.speed, { splitNumber: 2 }),
      series: [{ ...area(palette()[0], 0.35), name: "Throughput", data: s, animation: false }],
    };
  });
  const csvDaily = () => daily.map((d) => ({ day: d.day, served_bytes: d.served, downloads: d.downloads, update_checks: d.checks, players: d.players }));
</script>

{#if sum.error}<Alert tone="err" title="Could not load statistics">{sum.error}</Alert>{/if}

<div class="grid4">
  <Stat label="Served" icon={HardDriveDownload} value={c?.served ?? 0} format={bytes} spark={daily.map((d) => d.served)} delta={c ? change(c.served, p.served) : null} />
  {#if sum.data?.players_enabled !== false}
    <Stat label="Players" icon={Users} value={c?.players ?? 0} spark={daily.map((d) => d.players)} delta={c ? change(c.players, p.players) : null} />
  {/if}
  <Stat label="Full downloads" icon={Download} value={c?.files ?? 0} spark={daily.map((d) => d.downloads)} delta={c ? change(c.files, p.files) : null} />
  <Stat label="Update checks" icon={RefreshCw} value={c?.checks ?? 0} spark={daily.map((d) => d.checks)} delta={c ? change(c.checks, p.checks) : null} />
</div>

<section class="card live">
  <div class="lh">
    <span class="dot accent pulse"></span>
    <h2>Right now</h2>
    <span class="xs subtle">last 15 minutes · updates every 3 s</span>
  </div>
  <div class="lgrid">
    <div class="nums">
      <div><span class="eyebrow row tight"><Gauge size={13} />Throughput</span><b>{fmt.speed(live?.throughput ?? 0)}</b></div>
      <div><span class="eyebrow row tight"><Radio size={13} />Downloads</span><b>{live?.downloads ?? 0}{#if live?.max_downloads}<span class="subtle small"> / {live.max_downloads}</span>{/if}</b></div>
      {#if live?.players != null}<div><span class="eyebrow row tight"><Users size={13} />Active players</span><b>{live.players}</b></div>{/if}
      <div><span class="eyebrow row tight"><Timer size={13} />Avg speed ({days} d)</span><b>{fmt.speed(c?.avg_speed ?? 0)}</b></div>
    </div>
    <div class="lchart">
      {#if (live?.series ?? []).length > 1}
        <Chart option={liveOpt} height={120} label="Live throughput" />
      {:else}
        <p class="muted small">Collecting samples…</p>
      {/if}
    </div>
  </div>
</section>

<div class="cols">
  <ChartCard title="Traffic" description={`Data served${repo ? ` from ${repo}` : ""}, by type`} loading={tr.loading && !tr.data} empty={!!tr.data && !tr.data.total.served} csv={() => tr.data.series.map((r: any) => ({ time: new Date(r.t).toISOString(), content: r.bytes, delta_files: r.bytes_sidecar, metadata: r.bytes_meta }))} csvName="traffic">
    <Chart option={trafficOpt} height={280} label="Traffic chart" />
  </ChartCard>
  {#if sum.data?.players_enabled !== false}
    <ChartCard title="Players" description="Unique players per day, and over rolling 7 / 30 days" loading={pl.loading && !pl.data} empty={!!pl.data && !pl.data.total} csv={() => pl.data.daily} csvName="players">
      <Chart option={playersOpt} height={280} label="Players chart" />
    </ChartCard>
  {/if}
</div>

{#if c}
  <div class="grid4 small-stats">
    <Stat label="Requests" value={c.requests} delta={change(c.requests, p.requests)} />
    <Stat label="Partial requests" value={c.ranges} hint="resumes and delta updates" />
    <Stat label="Aborted" value={c.aborted_rate * 100} format={(v) => `${v.toFixed(1)}%`} invert delta={change(c.aborted, p.aborted)} tone={c.aborted_rate > 0.1 ? "warn" : undefined} />
    <Stat label="Errors" value={c.s404 + c.s401 + c.s4xx + c.s5xx + c.s503} invert delta={change(c.s404 + c.s401 + c.s4xx + c.s5xx + c.s503, p.s404 + p.s401 + p.s4xx + p.s5xx + p.s503)} />
  </div>
{/if}
<div class="row end"><button class="btn ghost sm" onclick={() => downloadCsv("daily-summary", csvDaily())}>Export daily summary (CSV)</button></div>

<style>
  .grid4 { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; }
  .small-stats :global(.val) { font-size: 24px; }
  .cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); gap: 16px; }
  @media (max-width: 520px) { .cols { grid-template-columns: 1fr; } }
  .live { display: flex; flex-direction: column; gap: 10px; background-image: radial-gradient(ellipse at 0% 0%, var(--accent-soft), transparent 55%); }
  .lh { display: flex; align-items: center; gap: 10px; }
  .lh h2 { margin: 0; font-size: 15.5px; }
  .lgrid { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 2fr); gap: 18px; align-items: center; }
  @media (max-width: 900px) { .lgrid { grid-template-columns: 1fr; } }
  .nums { display: grid; grid-template-columns: 1fr 1fr; gap: 12px 18px; }
  .nums > div { display: flex; flex-direction: column; gap: 2px; }
  .nums b { font-family: var(--font-display); font-size: 24px; font-weight: 600; font-variant-numeric: tabular-nums; }
</style>
