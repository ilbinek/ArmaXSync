<script lang="ts">
  import { onMount } from "svelte";
  import { Tabs, DateRangePicker, router, setQuery, type DateRange } from "@axs/kit";
  import { LayoutDashboard, Activity, Users, Package, ServerCog } from "@lucide/svelte";
  import { get, type RepoSummary } from "../../api";
  import Overview from "./Overview.svelte";
  import Traffic from "./Traffic.svelte";
  import Players from "./Players.svelte";
  import Addons from "./Addons.svelte";
  import Jobs from "./Jobs.svelte";

  /** Fixed repository (repository page) or a selectable one (Statistics page). */
  let { repo: fixedRepo = "", tabParam = "view" }: { repo?: string; tabParam?: string } = $props();

  const DAY = 86_400_000;
  const presetDays: Record<string, number> = { "24h": 1, "7d": 7, "30d": 30, "90d": 90, "1y": 365 };

  function initialRange(): DateRange {
    const p = router.query.get("range") ?? "30d";
    const from = Number(router.query.get("from"));
    const to = Number(router.query.get("to"));
    if (p === "custom" && from && to) return { from, to, preset: "custom" };
    const days = presetDays[p] ?? 30;
    const now = Date.now();
    return { from: now - days * DAY, to: now, preset: presetDays[p] ? p : "30d" };
  }

  let range = $state<DateRange>(initialRange());
  // svelte-ignore state_referenced_locally
  let repo = $state(fixedRepo || router.query.get("repo") || "");
  let tab = $state<string>();
  let repos = $state<RepoSummary[]>([]);
  let playersEnabled = $state(true);

  onMount(async () => {
    if (!fixedRepo) repos = await get<RepoSummary[]>("/repos").catch(() => []);
  });

  function changeRange(r: DateRange) {
    setQuery("range", r.preset === "30d" ? null : r.preset);
    setQuery("from", r.preset === "custom" ? String(r.from) : null);
    setQuery("to", r.preset === "custom" ? String(r.to) : null);
  }

  /** Query string shared by every widget. */
  const q = $derived(
    `from=${Math.round(range.from)}&to=${Math.round(range.to)}${repo ? `&repo=${encodeURIComponent(repo)}` : ""}&tz=${-new Date().getTimezoneOffset()}`,
  );
  const days = $derived(Math.max(1, Math.round((range.to - range.from) / DAY)));
</script>

<div class="bar">
  <Tabs
    bind:value={tab}
    param={tabParam}
    items={[
      { id: "overview", label: "Overview", icon: LayoutDashboard },
      { id: "traffic", label: "Traffic", icon: Activity },
      { id: "players", label: "Players", icon: Users, hidden: !playersEnabled },
      { id: "addons", label: "Addons", icon: Package },
      { id: "jobs", label: fixedRepo ? "Revisions & jobs" : "Repositories & jobs", icon: ServerCog },
    ]}
  />
  <div class="row filters">
    {#if !fixedRepo && repos.length > 1}
      <select bind:value={repo} onchange={() => setQuery("repo", repo || null)} aria-label="Repository" style="width:auto">
        <option value="">All repositories</option>
        {#each repos as r}<option value={r.slug}>{r.name}</option>{/each}
      </select>
    {/if}
    <DateRangePicker bind:value={range} onchange={changeRange} />
  </div>
</div>

{#key tab}
  <div class="view">
    {#if tab === "overview"}
      <Overview {q} {days} {repo} onplayers={(e) => (playersEnabled = e)} />
    {:else if tab === "traffic"}
      <Traffic {q} {days} {repo} />
    {:else if tab === "players"}
      <Players {q} {days} {repo} />
    {:else if tab === "addons"}
      <Addons {q} {days} {repo} />
    {:else if tab === "jobs"}
      <Jobs {q} {repo} />
    {/if}
  </div>
{/key}

<style>
  .bar { display: flex; flex-direction: column; gap: 12px; }
  .filters { justify-content: flex-end; }
  .view { display: flex; flex-direction: column; gap: 16px; animation: in 220ms var(--ease); }
  @keyframes in { from { opacity: 0; transform: translateY(6px); } }
</style>
