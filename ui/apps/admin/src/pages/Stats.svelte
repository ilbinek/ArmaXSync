<script lang="ts">
  import { PageHeader, Skeleton } from "@axs/kit";
  // Charts load on demand (keeps ECharts out of the main bundle).
  const view = import("../components/stats/StatsView.svelte");
</script>

<div class="page wide">
  <PageHeader title="Statistics" subtitle="Traffic, players, addons and publishing on this server. Times are shown in your local time zone." />
  {#await view}
    <Skeleton block height={44} /><Skeleton block height={320} />
  {:then m}
    <m.default />
  {/await}
</div>

<style>
  .page.wide { max-width: 1400px; }
</style>
