<script lang="ts">
  import { PageHeader } from "@axs/kit";
  import JobList from "../components/JobList.svelte";
</script>

<div class="page">
  <PageHeader title="Jobs" subtitle="Builds, publishes, mirror pulls and maintenance. Click a job for its log." />
  <div class="card flush"><JobList /></div>
</div>
