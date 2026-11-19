<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Section, Stat, BarChart, DataTable, SegmentedControl, Skeleton, Alert, bytes, errorText } from "@axs/kit";
  import { Activity, Download, FileDown, Globe } from "@lucide/svelte";
  import { get } from "../api";

  type Day = { day: string; repo: string; bytes: number; requests: number; files: number };
  type Total = { repo: string; bytes: number; requests: number; files: number };
  let data = $state<{ since_start: Total[]; daily: Day[] } | null>(null);
  let error = $state<string | null>(null);
  let range = $state<"7" | "30" | "60">("30");
  let repo = $state("");

  onMount(async () => {
    try {
      data = await get("/stats");
    } catch (e) {
      error = errorText(e);
    }
  });

  const n = $derived(Number(range));
  const repos = $derived([...new Set((data?.daily ?? []).map((d) => d.repo))].sort());
  const rows = $derived((data?.daily ?? []).filter((d) => !repo || d.repo === repo));
  const days = $derived.by(() => {
    const m = new Map<string, number>();
    for (const d of rows) m.set(d.day, (m.get(d.day) ?? 0) + d.bytes);
    return Array.from({ length: n }, (_, i) => {
      const day = new Date(Date.now() - (n - 1 - i) * 86400000).toISOString().slice(0, 10);
      return { label: day.slice(5), value: m.get(day) ?? 0, day };
    });
  });
  const cutoff = $derived(days[0]?.day ?? "");
  const perRepo = $derived.by(() => {
    const m = new Map<string, Total>();
    for (const d of data?.daily ?? []) {
      if (d.day < cutoff) continue;
      const e = m.get(d.repo) ?? { repo: d.repo, bytes: 0, files: 0, requests: 0 };
      e.bytes += d.bytes;
      e.files += d.files;
      e.requests += d.requests;
      m.set(d.repo, e);
    }
    return [...m.values()];
  });
  const sum = $derived(rows.filter((d) => d.day >= cutoff).reduce((a, d) => ({ bytes: a.bytes + d.bytes, files: a.files + d.files, requests: a.requests + d.requests }), { bytes: 0, files: 0, requests: 0 }));
  const live = $derived((data?.since_start ?? []).reduce((a, d) => a + d.bytes, 0));
</script>

<div class="page">
  <PageHeader title="Statistics" subtitle="Traffic served to players. Daily numbers are written every minute." />
  {#if error}
    <Alert tone="err" title="Could not load statistics">{error}</Alert>
  {:else if !data}
    <Skeleton block height={260} />
  {:else}
    <div class="row">
      <SegmentedControl bind:value={range} options={[{ value: "7", label: "7 days" }, { value: "30", label: "30 days" }, { value: "60", label: "60 days" }]} />
      <select bind:value={repo} style="width:auto" aria-label="Repository">
        <option value="">All repositories</option>
        {#each repos as r}<option value={r}>{r}</option>{/each}
      </select>
    </div>
    <div class="grid3">
      <Stat label="Served" value={bytes(sum.bytes)} icon={Download} hint={`last ${n} days`} />
      <Stat label="Full file downloads" value={sum.files.toLocaleString()} icon={FileDown} hint="complete files, not partial ranges" />
      <Stat label="Requests" value={sum.requests.toLocaleString()} icon={Globe} />
      <Stat label="Since restart" value={bytes(live)} icon={Activity} hint="all repositories" />
    </div>
    <Section title="Served per day" icon={Activity}>
      <BarChart data={days} format={bytes} height={220} label={`Hover a day · last ${n} days`} />
    </Section>
    <div class="card flush">
      <DataTable
        rows={perRepo}
        key={(r) => r.repo}
        sortBy="bytes"
        sortDesc
        columns={[
          { key: "repo", label: "Repository", sort: (r: Total) => r.repo },
          { key: "bytes", label: `Served (${n} d)`, sort: (r: Total) => r.bytes, align: "right" },
          { key: "files", label: "Full downloads", sort: (r: Total) => r.files, align: "right" },
          { key: "requests", label: "Requests", sort: (r: Total) => r.requests, align: "right", hideSm: true },
        ]}
        emptyTitle="No traffic in this period"
      >
        {#snippet cell(r, c)}
          {#if c.key === "repo"}<a href={`#/repos/${encodeURIComponent(r.repo)}`}>{r.repo}</a>
          {:else if c.key === "bytes"}<span class="tabular">{bytes(r.bytes)}</span>
          {:else if c.key === "files"}<span class="tabular">{r.files.toLocaleString()}</span>
          {:else}<span class="tabular">{r.requests.toLocaleString()}</span>{/if}
        {/snippet}
      </DataTable>
    </div>
  {/if}
</div>
