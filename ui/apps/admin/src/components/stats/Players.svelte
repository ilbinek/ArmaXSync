<script lang="ts">
  import { Stat, Alert, DataTable, Badge } from "@axs/kit";
  import { Users, UserPlus, MonitorSmartphone, Database, ShieldCheck } from "@lucide/svelte";
  import Chart from "../charts/Chart.svelte";
  import ChartCard from "../charts/ChartCard.svelte";
  import { area, axisTooltip, fmt, valueAxis, dayLabel, palette, type EChartsOption } from "../charts/echarts";
  import { loader } from "./load.svelte";

  let { q, days, repo }: { q: string; days: number; repo: string } = $props();

  const pl = loader<any>(() => `/stats/players?${q}`);
  const cl = loader<any>(() => `/stats/clients?${q}`);

  const d = $derived((pl.data?.daily ?? []) as any[]);
  const peak = $derived(d.reduce((m, r) => (r.players > m.players ? r : m), { players: 0, day: "" }));
  const avg = $derived(d.length ? d.reduce((s, r) => s + r.players, 0) / d.length : 0);
  // Players are recognised within a calendar month only (monthly salt).
  const thisMonth = new Date().toISOString().slice(0, 7);
  const newThisMonth = $derived(d.filter((r) => r.day.startsWith(thisMonth)).reduce((s, r) => s + r.new, 0));

  const newRetOpt = $derived.by((): EChartsOption => {
    const pal = palette();
    return {
      tooltip: axisTooltip(fmt.num),
      legend: { data: ["Returning", "New this month"] },
      xAxis: { type: "category", data: d.map((r) => dayLabel(r.day)) },
      yAxis: valueAxis(fmt.num, { minInterval: 1 }),
      series: [
        { type: "bar", name: "Returning", stack: "p", data: d.map((r) => r.returning), itemStyle: { color: pal[1] }, barMaxWidth: 22 },
        { type: "bar", name: "New this month", stack: "p", data: d.map((r) => r.new), itemStyle: { color: pal[0], borderRadius: [3, 3, 0, 0] }, barMaxWidth: 22 },
      ],
    };
  });

  const rollOpt = $derived.by((): EChartsOption => {
    const [a, b, c] = palette();
    return {
      tooltip: axisTooltip(fmt.num),
      legend: { data: ["Daily", "Weekly", "Monthly"] },
      xAxis: { type: "category", data: d.map((r) => dayLabel(r.day)), boundaryGap: false },
      yAxis: valueAxis(fmt.num, { minInterval: 1 }),
      series: [
        { ...area(a, 0.25), name: "Daily", data: d.map((r) => r.players) },
        { type: "line", name: "Weekly", smooth: 0.25, showSymbol: false, data: d.map((r) => r.weekly), itemStyle: { color: b } },
        { type: "line", name: "Monthly", smooth: 0.25, showSymbol: false, data: d.map((r) => r.monthly), itemStyle: { color: c }, lineStyle: { type: "dashed" } },
      ],
    };
  });

  const perRepoOpt = $derived.by((): EChartsOption => {
    const rows = [...((pl.data?.per_repo ?? []) as any[])].reverse();
    return {
      grid: { left: 8, right: 30, top: 8, bottom: 8, containLabel: true },
      tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
      xAxis: valueAxis(fmt.num, { minInterval: 1 }),
      yAxis: { type: "category", data: rows.map((r) => r.repo), splitLine: { show: false } },
      series: [{ type: "bar", name: "Players", data: rows.map((r) => r.players), itemStyle: { color: palette()[0], borderRadius: [0, 3, 3, 0] }, barMaxWidth: 20, label: { show: true, position: "right", color: "inherit", fontSize: 11 } }],
    };
  });

  const families = [
    ["armaxsync", "ArmaXSync"],
    ["arma3sync", "ArmA3Sync (legacy)"],
    ["browser", "Browser"],
    ["other", "Other"],
  ] as const;
  const famTotals = $derived(
    families.map(([k, label]) => ({ name: label, value: ((cl.data?.daily ?? []) as any[]).reduce((s, r) => s + (r[k] ?? 0), 0) })),
  );
  const donutOpt = $derived.by((): EChartsOption => ({
    tooltip: { trigger: "item", formatter: (p: any) => `${p.marker}${p.name}<br/><b>${fmt.num(p.value)}</b> requests (${p.percent}%)` },
    legend: { orient: "vertical", left: 0, top: "middle" },
    series: [{ type: "pie", radius: ["55%", "80%"], center: ["68%", "50%"], avoidLabelOverlap: true, label: { show: false }, itemStyle: { borderColor: "transparent", borderWidth: 2, borderRadius: 4 }, data: famTotals }],
  }));
  const famOpt = $derived.by((): EChartsOption => {
    const rows = (cl.data?.daily ?? []) as any[];
    const pal = palette();
    return {
      tooltip: axisTooltip(fmt.num),
      legend: { data: families.map((f) => f[1]) },
      xAxis: { type: "category", data: rows.map((r) => dayLabel(r.day)), boundaryGap: false },
      yAxis: valueAxis(fmt.num),
      series: families.map(([k, label], i) => ({ ...area(pal[i], 0.4, true), name: label, stack: "f", data: rows.map((r) => r[k] ?? 0) })),
    };
  });

  const newest = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });
  const versions = $derived(((cl.data?.versions ?? []) as any[]).filter((v) => v.family === "armaxsync").sort((a, b) => newest(b.version, a.version)));
  const latest = $derived(versions[0]?.version);
</script>

{#if pl.data && pl.data.enabled === false}
  <Alert tone="info" title="Player counting is off">Enable <code>players = true</code> under <code>[stats]</code> in the server configuration.</Alert>
{:else}
  <Alert tone="info" icon={ShieldCheck}>Players are counted from a monthly-salted hash of their address; addresses are never stored. One address = one player, so players sharing a connection count once.</Alert>
  <div class="grid4">
    <Stat label="Players" icon={Users} value={pl.data?.total ?? 0} hint={`unique in ${days} day${days > 1 ? "s" : ""}${repo ? ` · ${repo}` : ""}`} />
    <Stat label="Average per day" value={avg} format={(v) => v.toFixed(1)} spark={d.map((r) => r.players)} />
    <Stat label="Busiest day" value={peak.players} hint={peak.day ? new Date(peak.day + "T00:00:00Z").toLocaleDateString(undefined, { weekday: "long", month: "short", day: "numeric", timeZone: "UTC" }) : ""} />
    <Stat label="New this month" icon={UserPlus} value={newThisMonth} hint="not seen earlier this month" />
  </div>

  <div class="cols">
    <ChartCard title="Active players" icon={Users} description="Unique per day, and over rolling 7 and 30 days" loading={pl.loading && !pl.data} empty={!!pl.data && !pl.data.total} csv={() => d} csvName="players">
      <Chart option={rollOpt} label="Active players" />
    </ChartCard>
    <ChartCard title="New vs. returning" icon={UserPlus} description="Players can only be recognised within a calendar month, so everyone counts as new again on the 1st" loading={pl.loading && !pl.data} empty={!!pl.data && !pl.data.total} csv={() => d.map((r) => ({ day: r.day, new: r.new, returning: r.returning }))} csvName="new-returning">
      <Chart option={newRetOpt} label="New vs returning players" />
    </ChartCard>
    {#if !repo && (pl.data?.per_repo ?? []).length > 1}
      <ChartCard title="Players per repository" icon={Database} loading={pl.loading && !pl.data} csv={() => pl.data.per_repo} csvName="players-per-repository" height={Math.max(160, pl.data.per_repo.length * 34)}>
        <Chart option={perRepoOpt} height={Math.max(160, pl.data.per_repo.length * 34)} label="Players per repository" />
      </ChartCard>
    {/if}
  </div>
{/if}

<div class="cols">
  <ChartCard title="Clients" icon={MonitorSmartphone} description="Requests by client program" loading={cl.loading && !cl.data} empty={!!cl.data && !famTotals.some((f) => f.value)} csv={() => famTotals} csvName="clients">
    <Chart option={donutOpt} height={220} label="Clients" />
  </ChartCard>
  <ChartCard title="Clients over time" icon={MonitorSmartphone} loading={cl.loading && !cl.data} empty={!!cl.data && !famTotals.some((f) => f.value)} csv={() => cl.data.daily} csvName="clients-daily">
    <Chart option={famOpt} height={220} label="Clients over time" />
  </ChartCard>
</div>

<section class="card flush">
  <div class="vh"><h2>ArmaXSync versions</h2><span class="small muted">Requests per client version. Players on old versions update automatically if they allowed it.</span></div>
  <DataTable
    rows={versions}
    key={(v) => v.version}
    columns={[
      { key: "version", label: "Version" },
      { key: "requests", label: "Requests", align: "right" },
      { key: "last", label: "Last seen", hideSm: true },
    ]}
    emptyTitle="No ArmaXSync clients yet"
    loading={cl.loading && !cl.data}
  >
    {#snippet cell(v, c)}
      {#if c.key === "version"}
        <span class="row tight"><span class="mono">{v.version}</span>{#if v.version === latest}<Badge tone="ok">Newest seen</Badge>{:else}<Badge tone="warn" plain>Older</Badge>{/if}</span>
      {:else if c.key === "requests"}<span class="tabular">{v.requests.toLocaleString()}</span>
      {:else}<span class="small muted">{v.last_seen}</span>{/if}
    {/snippet}
  </DataTable>
</section>

<style>
  .grid4 { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; }
  .grid4 :global(.val) { font-size: 26px; }
  .cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); gap: 16px; }
  @media (max-width: 520px) { .cols { grid-template-columns: 1fr; } }
  .vh { display: flex; flex-direction: column; gap: 3px; padding: 14px 16px 10px; border-bottom: 1px solid var(--line); }
  .vh h2 { margin: 0; font-size: 15.5px; }
</style>
