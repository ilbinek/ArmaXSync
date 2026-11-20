<script lang="ts">
  import { DataTable, Badge, Stat, ago, bytes } from "@axs/kit";
  import { HardDrive, CalendarCheck, ListChecks, CircleAlert } from "@lucide/svelte";
  import { jobLabel } from "../../api";
  import { openJob } from "../../session.svelte";
  import Chart from "../charts/Chart.svelte";
  import ChartCard from "../charts/ChartCard.svelte";
  import { axisTooltip, fmt, valueAxis, palette, type EChartsOption } from "../charts/echarts";
  import { loader } from "./load.svelte";

  let { q, repo }: { q: string; repo: string } = $props();
  const rp = loader<any>(() => `/stats/repos?${q}`);

  const revisions = $derived((rp.data?.revisions ?? {}) as Record<string, any[]>);
  const names = $derived(Object.keys(revisions));
  const jobs = $derived(Object.entries((rp.data?.jobs ?? {}) as Record<string, any>).map(([kind, v]) => ({ kind, ...v })));
  const pubs = $derived(jobs.filter((j) => ["publish", "build", "mirror"].includes(j.kind)).reduce((s, j) => s + j.succeeded, 0));
  const failed = $derived(jobs.reduce((s, j) => s + j.failed, 0));
  const total = $derived(jobs.reduce((s, j) => s + j.total, 0));
  const size = $derived(((rp.data?.current ?? []) as any[]).reduce((s, r) => s + r.total_size, 0));

  const sizeOpt = $derived.by((): EChartsOption => {
    const pal = palette();
    return {
      tooltip: {
        trigger: "item",
        formatter: (p: any) => `${p.marker}<b>${p.seriesName}</b> revision ${p.data.rev}<br/>${new Date(p.value[0]).toLocaleString()}<br/>${bytes(p.value[1])} · ${p.data.files?.toLocaleString() ?? "?"} files`,
      },
      legend: names.length > 1 ? { data: names, type: "scroll" } : undefined,
      xAxis: { type: "time" },
      yAxis: valueAxis(fmt.bytes, { scale: true }),
      series: names.map((n, i) => ({
        type: "line",
        name: n,
        step: "end",
        symbolSize: 6,
        itemStyle: { color: pal[i % pal.length] },
        data: revisions[n].filter((r) => r.total_size != null).map((r) => ({ value: [r.at, r.total_size], rev: r.revision, files: r.files })),
      })),
    };
  });

  const weeklyOpt = $derived.by((): EChartsOption => {
    const w = (rp.data?.publishes_weekly ?? []) as any[];
    return {
      tooltip: axisTooltip(fmt.num),
      xAxis: { type: "category", data: w.map((r) => new Date(r.t).toLocaleDateString(undefined, { month: "short", day: "numeric", timeZone: "UTC" })) },
      yAxis: valueAxis(fmt.num, { minInterval: 1 }),
      series: [{ type: "bar", name: "Published revisions", data: w.map((r) => r.count), itemStyle: { color: palette()[0], borderRadius: [3, 3, 0, 0] }, barMaxWidth: 26 }],
    };
  });
  const revRows = $derived(
    names.flatMap((n) => revisions[n].map((r) => ({ repo: n, ...r }))).sort((a, b) => b.at - a.at),
  );
</script>

<div class="grid4">
  <Stat label="Hosted" icon={HardDrive} value={size} format={bytes} hint={repo ? "current revision" : "all repositories, current revisions"} />
  <Stat label="Revisions published" icon={CalendarCheck} value={pubs} />
  <Stat label="Jobs" icon={ListChecks} value={total} hint={total ? `${(((total - failed) / total) * 100).toFixed(0)}% succeeded` : undefined} />
  <Stat label="Failed jobs" icon={CircleAlert} value={failed} tone={failed ? "err" : undefined} />
</div>

<div class="cols">
  <ChartCard title="Repository size" icon={HardDrive} description="Content size after each published revision" loading={rp.loading && !rp.data} empty={!!rp.data && !revRows.some((r) => r.total_size != null)} emptyText="No revisions published in this period." csv={() => revRows} csvName="revisions">
    <Chart option={sizeOpt} label="Repository size over time" />
  </ChartCard>
  <ChartCard title="Publishing activity" icon={CalendarCheck} description="Revisions published per week" loading={rp.loading && !rp.data} empty={!!rp.data && !(rp.data.publishes_weekly ?? []).length} csv={() => rp.data.publishes_weekly.map((r: any) => ({ week: new Date(r.t).toISOString().slice(0, 10), revisions: r.count }))} csvName="publishes-weekly">
    <Chart option={weeklyOpt} label="Publishes per week" />
  </ChartCard>
</div>

<section class="card flush">
  <div class="vh"><h2>Jobs by kind</h2><span class="small muted">Durations are run time; waiting is time queued before a worker was free.</span></div>
  <DataTable
    rows={jobs}
    key={(j) => j.kind}
    loading={rp.loading && !rp.data}
    sortBy="total"
    sortDesc
    emptyTitle="No jobs in this period"
    columns={[
      { key: "kind", label: "Kind" },
      { key: "total", label: "Runs", sort: (j: any) => j.total, align: "right" },
      { key: "rate", label: "Success", sort: (j: any) => (j.total ? j.succeeded / j.total : 0), align: "right" },
      { key: "avg", label: "Avg duration", sort: (j: any) => j.avg_ms, align: "right" },
      { key: "p95", label: "95th percentile", sort: (j: any) => j.p95_ms, align: "right", hideSm: true },
      { key: "wait", label: "Avg wait", sort: (j: any) => j.avg_wait_ms, align: "right", hideSm: true },
    ]}
  >
    {#snippet cell(j, c)}
      {#if c.key === "kind"}<span class="strong">{jobLabel(j.kind)}</span>
      {:else if c.key === "total"}<span class="tabular">{j.total}</span>
      {:else if c.key === "rate"}
        {@const r = j.total ? j.succeeded / j.total : 1}
        <Badge tone={r >= 0.98 ? "ok" : r >= 0.8 ? "warn" : "err"} plain>{(r * 100).toFixed(0)}%</Badge>
      {:else if c.key === "avg"}<span class="tabular small">{fmt.ms(j.avg_ms)}</span>
      {:else if c.key === "p95"}<span class="tabular small">{fmt.ms(j.p95_ms)}</span>
      {:else}<span class="tabular small muted">{fmt.ms(j.avg_wait_ms)}</span>{/if}
    {/snippet}
  </DataTable>
</section>

{#if (rp.data?.failures ?? []).length}
  <section class="card flush">
    <div class="vh"><h2>Recent failures</h2></div>
    <ul class="fails">
      {#each rp.data.failures as f}
        <li>
          <button onclick={() => openJob(f.id)}>
            <CircleAlert size={15} />
            <span class="grow" style="min-width:0">
              <span class="strong small">{jobLabel(f.kind)} #{f.id}{f.repo ? ` · ${f.repo}` : ""}</span>
              <span class="xs err-text ellipsis" style="display:block">{f.error ?? "failed"}</span>
            </span>
            <span class="xs subtle nowrap">{ago(f.at)}</span>
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .grid4 { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; }
  .grid4 :global(.val) { font-size: 26px; }
  .cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); gap: 16px; }
  @media (max-width: 520px) { .cols { grid-template-columns: 1fr; } }
  .vh { display: flex; flex-direction: column; gap: 3px; padding: 14px 16px 10px; border-bottom: 1px solid var(--line); }
  .vh h2 { margin: 0; font-size: 15.5px; }
  .fails { list-style: none; margin: 0; padding: 0; }
  .fails li + li { border-top: 1px solid var(--line); }
  .fails button { display: flex; align-items: center; gap: 10px; width: 100%; padding: 10px 16px; background: none; border: 0; color: var(--fg); font: inherit; text-align: left; cursor: pointer; }
  .fails button:hover { background: var(--surface-2); }
  .fails :global(svg) { color: var(--err); flex-shrink: 0; }
</style>
