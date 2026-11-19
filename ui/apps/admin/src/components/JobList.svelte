<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { DataTable, Badge, Button, SegmentedControl, ago, date, duration, errorText } from "@axs/kit";
  import { ListChecks } from "@lucide/svelte";
  import { get, jobLabel, type Job } from "../api";
  import { openJob, session } from "../session.svelte";

  let { repo = "", limit = 50, compact = false }: { repo?: string; limit?: number; compact?: boolean } = $props();

  let jobs = $state<Job[] | null>(null);
  let error = $state<string | null>(null);
  let status = $state<"" | "active" | "failed" | "succeeded">("");
  let more = $state(false);
  let timer: ReturnType<typeof setInterval>;

  async function load(append = false) {
    try {
      const before = append && jobs?.length ? `&before=${jobs[jobs.length - 1].id}` : "";
      const page = await get<Job[]>(`/jobs?limit=${limit}${repo ? `&repo=${encodeURIComponent(repo)}` : ""}${status ? `&status=${status}` : ""}${before}`);
      jobs = append && jobs ? [...jobs, ...page] : page;
      more = !compact && page.length === limit;
      error = null;
    } catch (e) {
      error = errorText(e);
    }
  }
  onMount(() => {
    load();
    // Refresh while visible; finished jobs also refresh via session.jobsDone.
    timer = setInterval(() => !document.hidden && (!jobs || jobs.length <= limit) && load(), 5000);
  });
  onDestroy(() => clearInterval(timer));
  let seen = session.jobsDone;
  $effect(() => {
    if (session.jobsDone !== seen) {
      seen = session.jobsDone;
      load();
    }
  });

  const tone = (s: string) => (s === "succeeded" ? "ok" : s === "failed" || s === "interrupted" ? "err" : s === "running" ? "accent" : s === "cancelled" ? "neutral" : "warn");
  const took = (j: Job) => (j.started_at && j.finished_at ? duration((j.finished_at - j.started_at) / 1000) : j.started_at ? "running" : "");
  const columns = $derived([
    { key: "job", label: "Job" },
    ...(repo ? [] : [{ key: "repo", label: "Repository", hideSm: true }]),
    { key: "status", label: "Status", width: "130px" },
    ...(compact ? [] : [{ key: "started", label: "Started", width: "150px", hideSm: true }, { key: "took", label: "Took", width: "90px", hideSm: true }]),
  ]);
</script>

{#snippet filters()}
  <SegmentedControl size="sm" bind:value={status} onchange={() => load()} options={[{ value: "", label: "All" }, { value: "active", label: "Running" }, { value: "failed", label: "Failed" }, { value: "succeeded", label: "Succeeded" }]} />
{/snippet}

<DataTable rows={jobs} {columns} loading={!jobs} {error} onretry={() => load()} key={(j) => j.id} onrowclick={(j) => openJob(j.id)} emptyIcon={ListChecks} emptyTitle="No jobs" emptyText="Builds, publishes and mirror pulls appear here." dense={compact} toolbar={compact ? undefined : filters}>
  {#snippet cell(j, c)}
    {#if c.key === "job"}
      <div class="stack" style="gap:1px;min-width:0">
        <span><span class="strong">{jobLabel(j.kind)}</span> <span class="subtle xs">#{j.id}</span>{#if j.summary?.revision}<span class="muted small">&nbsp;→ revision {j.summary.revision}</span>{/if}</span>
        {#if j.error && j.status !== "succeeded"}<span class="xs err-text ellipsis" title={j.error} style="max-width:520px">{j.error}</span>{/if}
        {#if compact}<span class="xs subtle">{ago(j.created_at)}</span>{/if}
      </div>
    {:else if c.key === "repo"}<span class="small">{j.repo ?? "–"}</span>
    {:else if c.key === "status"}<Badge tone={tone(j.status)} dot pulse={j.status === "running"}>{j.status}</Badge>
    {:else if c.key === "started"}<span class="small muted" title={date(j.started_at ?? j.created_at)}>{ago(j.started_at ?? j.created_at)}</span>
    {:else if c.key === "took"}<span class="small muted tabular">{took(j)}</span>
    {/if}
  {/snippet}
</DataTable>
{#if more}<div class="row" style="justify-content:center;padding:10px"><Button size="sm" onclick={() => load(true)}>Older jobs</Button></div>{/if}
