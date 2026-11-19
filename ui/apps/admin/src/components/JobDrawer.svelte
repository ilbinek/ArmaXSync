<script lang="ts">
  import { Drawer, Progress, Badge, Button, Alert, Skeleton, bytes, date, duration, copyText, toast, confirmDialog, toastError } from "@axs/kit";
  import { Copy, Download, CircleStop } from "@lucide/svelte";
  import { get, post, followJob, jobLabel, type Job } from "../api";
  import { session } from "../session.svelte";

  let job = $state<Job | null>(null);
  let lines = $state<string[]>([]);
  let progress = $state<{ phase: string; done: number; total: number } | null>(null);
  let status = $state("queued");
  let logEl = $state<HTMLElement>();
  let open = $state(false);

  $effect(() => {
    const id = session.jobId;
    if (id == null) return;
    open = true;
    job = null;
    lines = [];
    progress = null;
    let stop: (() => void) | null = null;
    let alive = true;
    (async () => {
      try {
        const j = await get<Job>(`/jobs/${id}`);
        if (!alive) return;
        job = j;
        status = j.status;
        lines = (j.log ?? "").split("\n").filter(Boolean);
        if (status === "queued" || status === "running") {
          lines = [];
          stop = followJob(id, async (e) => {
            if (e.type === "log") lines.push(e.line);
            else if (e.type === "progress") progress = e;
            else if (e.type === "status") {
              status = e.status;
              if (status !== "running" && status !== "queued") {
                job = await get<Job>(`/jobs/${id}`);
                session.jobsDone++;
              }
            }
            queueMicrotask(() => logEl?.scrollTo(0, logEl.scrollHeight));
          });
        }
        queueMicrotask(() => logEl?.scrollTo(0, logEl.scrollHeight));
      } catch (e) {
        toastError(e);
      }
    })();
    return () => {
      alive = false;
      stop?.();
    };
  });

  $effect(() => {
    if (!open) session.jobId = null;
  });

  const tone = $derived(status === "succeeded" ? "ok" : status === "failed" || status === "interrupted" ? "err" : status === "running" ? "accent" : "warn");
  const lineClass = (l: string) => (/^error|failed|\berror:/i.test(l) ? "l-err" : /warn/i.test(l) ? "l-warn" : /published|done|verified|staged/i.test(l) ? "l-ok" : "");
  const progressLabel = $derived(
    progress ? `${progress.phase} · ${["download", "hashing", "upload"].includes(progress.phase) ? `${bytes(progress.done)} of ${bytes(progress.total)}` : `${progress.done} of ${progress.total}`}` : "",
  );

  async function cancel() {
    if (!job || !(await confirmDialog({ title: `Cancel job #${job.id}?`, message: "Changes it already made are rolled back where possible.", confirmLabel: "Cancel job", cancelLabel: "Keep running", danger: true }))) return;
    try {
      await post(`/jobs/${job.id}/cancel`);
    } catch (e) {
      toastError(e);
    }
  }

  function download() {
    const blob = new Blob([lines.join("\n")], { type: "text/plain" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `job-${job?.id}.log`;
    a.click();
    URL.revokeObjectURL(a.href);
  }
</script>

<Drawer bind:open title={job ? `${jobLabel(job.kind)} #${job.id}` : "Job"} subtitle={job?.repo ? `Repository ${job.repo}` : ""} width={640}>
  {#if !job}
    <Skeleton lines={6} />
  {:else}
    <div class="row">
      <Badge {tone} dot pulse={status === "running"}>{status}</Badge>
      <span class="small muted">Queued {date(job.created_at)}</span>
      {#if job.started_at && job.finished_at}<span class="small muted">· took {duration((job.finished_at - job.started_at) / 1000)}</span>{/if}
    </div>
    {#if status === "running" || status === "queued"}
      <Progress value={progress?.done ?? 0} max={progress?.total ?? 0} indeterminate={!progress?.total} label={progressLabel || (status === "queued" ? "Waiting for a worker…" : "Working…")} />
    {/if}
    {#if job.error && status !== "succeeded"}<Alert tone="err" title="Failed">{job.error}</Alert>{/if}
    {#if job.summary && status === "succeeded"}
      <Alert tone="ok" title="Done">
        {#if job.summary.revision}Revision {job.summary.revision} published · {job.summary.files} files, {bytes(job.summary.total_size)}
        {:else if job.summary.changed === false}Already up to date.
        {:else if job.summary.removed !== undefined}Removed {job.summary.removed} item(s).
        {:else if job.summary.staged}Update {job.summary.staged} staged; the server restarts shortly.
        {:else}Finished.{/if}
      </Alert>
    {/if}
    <div class="row between">
      <h3 style="margin:0">Log</h3>
      <div class="row tight">
        <Button size="sm" variant="ghost" icon={Copy} onclick={async () => (await copyText(lines.join("\n"))) && toast("Log copied")}>Copy</Button>
        <Button size="sm" variant="ghost" icon={Download} onclick={download}>Download</Button>
      </div>
    </div>
    <div class="log joblog" bind:this={logEl}>
      {#each lines as l}<div class={lineClass(l)}>{l}</div>{:else}<span class="subtle">No output yet.</span>{/each}
    </div>
  {/if}
  {#snippet footer()}
    {#if status === "running" || status === "queued"}<Button variant="danger" icon={CircleStop} onclick={cancel}>Cancel job</Button>{/if}
    <Button onclick={() => (open = false)}>Close</Button>
  {/snippet}
</Drawer>

<style>
  .joblog { max-height: none; flex: 1; min-height: 240px; }
</style>
