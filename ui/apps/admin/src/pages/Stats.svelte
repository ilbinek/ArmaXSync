<script lang="ts">
  import { onMount } from "svelte";
  import { bytes } from "@axs/kit";
  import { get } from "../api";

  let data = $state<any>(null);
  onMount(async () => {
    data = await get("/stats");
  });

  // Last 30 days, one bar per day, total bytes across repositories.
  const days = $derived.by(() => {
    if (!data) return [];
    const map = new Map<string, number>();
    for (const d of data.daily) map.set(d.day, (map.get(d.day) ?? 0) + d.bytes);
    const out: { day: string; bytes: number }[] = [];
    for (let i = 29; i >= 0; i--) {
      const day = new Date(Date.now() - i * 86400000).toISOString().slice(0, 10);
      out.push({ day, bytes: map.get(day) ?? 0 });
    }
    return out;
  });
  const max = $derived(Math.max(1, ...days.map((d) => d.bytes)));
  const perRepo = $derived.by(() => {
    if (!data) return [];
    const m = new Map<string, { bytes: number; files: number; requests: number }>();
    for (const d of data.daily) {
      const e = m.get(d.repo) ?? { bytes: 0, files: 0, requests: 0 };
      e.bytes += d.bytes;
      e.files += d.files;
      e.requests += d.requests;
      m.set(d.repo, e);
    }
    return [...m.entries()].sort((a, b) => b[1].bytes - a[1].bytes);
  });
</script>

<h1>Statistics</h1>
<div class="muted" style="margin-bottom:14px">Downloads served (last 60 days).</div>
{#if !data}
  <div class="empty">Loading…</div>
{:else}
  <div class="stack">
    <div class="card">
      <h2>Traffic, last 30 days</h2>
      <div class="chart" role="img" aria-label="Bytes served per day">
        {#each days as d}
          <div class="col" title={`${d.day}: ${bytes(d.bytes)}`}><div class="bar" style="height:{(d.bytes / max) * 100}%"></div></div>
        {/each}
      </div>
      <div class="row small muted"><span>{days[0]?.day}</span><div class="spacer"></div><span>peak {bytes(max)}/day</span><div class="spacer"></div><span>{days[days.length - 1]?.day}</span></div>
    </div>
    <div class="card flush">
      <table>
        <thead><tr><th>Repository</th><th>Served (60 days)</th><th>Full downloads</th><th>Requests</th></tr></thead>
        <tbody>
          {#each perRepo as [repo, s]}
            <tr><td>{repo}</td><td>{bytes(s.bytes)}</td><td>{s.files}</td><td>{s.requests}</td></tr>
          {:else}<tr><td colspan="4" class="empty">No downloads yet.</td></tr>{/each}
        </tbody>
      </table>
    </div>
  </div>
{/if}

<style>
  .chart { display: flex; align-items: flex-end; gap: 3px; height: 140px; margin-bottom: 6px; }
  .col { flex: 1; height: 100%; display: flex; align-items: flex-end; }
  .bar { width: 100%; background: var(--accent); border-radius: 3px 3px 0 0; min-height: 1px; }
</style>
