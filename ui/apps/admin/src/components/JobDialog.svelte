<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Modal, Progress, bytes } from "@axs/kit";
  import { get, post, followJob } from "../api";

  let { id, onclose, ondone }: { id: number; onclose: () => void; ondone?: () => void } = $props();
  let open = $state(true);
  let job = $state<any>(null);
  let lines = $state<string[]>([]);
  let progress: { phase: string; done: number; total: number } | null = $state(null);
  let status = $state("queued");
  let stop: (() => void) | null = null;
  let logEl: HTMLElement | undefined = $state();

  onMount(async () => {
    job = await get(`/jobs/${id}`);
    status = job.status;
    if (job.log) lines = job.log.split("\n").filter(Boolean);
    if (status === "queued" || status === "running") {
      lines = [];
      stop = followJob(id, async (e) => {
        if (e.type === "log") lines.push(e.line);
        else if (e.type === "progress") progress = e;
        else if (e.type === "status") {
          status = e.status;
          if (status !== "running" && status !== "queued") {
            job = await get(`/jobs/${id}`);
            ondone?.();
          }
        }
        queueMicrotask(() => logEl?.scrollTo(0, logEl.scrollHeight));
      });
    }
  });
  onDestroy(() => stop?.());

  $effect(() => {
    if (!open) onclose();
  });

  const badge = $derived(status === "succeeded" ? "ok" : status === "failed" ? "err" : status === "running" ? "info" : "warn");
</script>

<Modal bind:open title={`Job #${id}${job ? " · " + job.kind : ""}`} wide>
  <div class="row"><span class="badge {badge}">{status}</span>{#if job?.repo}<span class="muted">repository {job.repo}</span>{/if}</div>
  {#if progress && (status === "running" || status === "queued")}
    <Progress value={progress.done} max={progress.total} label={`${progress.phase} · ${progress.phase === "download" || progress.phase === "hashing" ? bytes(progress.done) + " / " + bytes(progress.total) : progress.done + " / " + progress.total}`} />
  {/if}
  {#if job?.error}<div class="alert err">{job.error}</div>{/if}
  {#if job?.summary && status === "succeeded"}
    <div class="alert ok">
      {#if job.summary.revision}Revision {job.summary.revision} published — {job.summary.files} files, {bytes(job.summary.total_size)}{/if}
      {#if job.summary.changed === false}Already up to date.{/if}
      {#if job.summary.removed !== undefined}Removed {job.summary.removed} item(s).{/if}
    </div>
  {/if}
  <div class="log" bind:this={logEl}>{lines.join("\n") || "…"}</div>
  {#snippet footer()}
    {#if status === "running" || status === "queued"}<button class="btn danger" onclick={() => post(`/jobs/${id}/cancel`)}>Cancel job</button>{/if}
    <button class="btn" onclick={() => (open = false)}>Close</button>
  {/snippet}
</Modal>
