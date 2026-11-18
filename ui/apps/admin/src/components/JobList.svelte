<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { date } from "@axs/kit";
  import { get } from "../api";

  let { repo = "", onopen }: { repo?: string; onopen: (id: number) => void } = $props();
  let jobs = $state<any[]>([]);
  let timer: any;

  async function load() {
    jobs = await get(`/jobs?limit=100${repo ? "&repo=" + encodeURIComponent(repo) : ""}`);
  }
  onMount(() => {
    load();
    timer = setInterval(load, 4000);
  });
  onDestroy(() => clearInterval(timer));

  const cls = (s: string) => (s === "succeeded" ? "ok" : s === "failed" || s === "interrupted" ? "err" : s === "running" ? "info" : "warn");
  function took(j: any) {
    if (!j.started_at || !j.finished_at) return "";
    const s = (j.finished_at - j.started_at) / 1000;
    return s < 60 ? `${s.toFixed(1)} s` : `${(s / 60).toFixed(1)} min`;
  }
</script>

{#if jobs.length === 0}
  <div class="card empty">No jobs yet.</div>
{:else}
  <div class="card flush">
    <table>
      <thead><tr><th>#</th><th>Job</th>{#if !repo}<th>Repository</th>{/if}<th>Status</th><th class="hide-sm">Started</th><th class="hide-sm">Duration</th></tr></thead>
      <tbody>
        {#each jobs as j}
          <tr class="clickable" onclick={() => onopen(j.id)}>
            <td>{j.id}</td>
            <td>{j.kind}{#if j.summary?.revision} <span class="muted small">→ rev {j.summary.revision}</span>{/if}</td>
            {#if !repo}<td>{j.repo ?? "-"}</td>{/if}
            <td><span class="badge {cls(j.status)}">{j.status}</span>{#if j.error}<div class="small muted" title={j.error}>{j.error.slice(0, 80)}</div>{/if}</td>
            <td class="hide-sm">{date(j.started_at ?? j.created_at)}</td>
            <td class="hide-sm">{took(j)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
