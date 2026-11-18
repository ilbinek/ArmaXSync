<script lang="ts">
  import JobList from "../components/JobList.svelte";
  import JobDialog from "../components/JobDialog.svelte";
  let jobId = $state<number | null>(null);
</script>

<h1>Jobs</h1>
<div class="muted" style="margin-bottom:14px">Builds, publishes, mirror pulls and housekeeping.</div>
<JobList onopen={(id: number) => (jobId = id)} />
{#if jobId}<JobDialog id={jobId} onclose={() => (jobId = null)} />{/if}
