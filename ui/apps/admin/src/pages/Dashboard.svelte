<script lang="ts">
  import { onMount } from "svelte";
  import { Stat, Section, Button, Badge, EmptyState, Skeleton, Sparkline, Topo, bytes, ago, navigate } from "@axs/kit";
  import { Plus, Database, HardDriveDownload, Users, Download, ServerCog, CircleAlert, Lock, Globe, Package, Radio, ArrowUpCircle, ChartColumn } from "@lucide/svelte";
  import { get, jobLabel, type RepoSummary, type Job } from "../api";
  import { session, isAdmin, openJob } from "../session.svelte";
  import CreateRepo from "../components/CreateRepo.svelte";
  import { change } from "../components/stats/load.svelte";

  const DAY = 86_400_000;
  let repos = $state<RepoSummary[] | null>(null);
  let recent = $state<Job[]>([]);
  let sum = $state<any>(null);
  let traffic = $state<any>(null);
  let addons = $state<any[]>([]);
  let live = $state<any>(null);
  let createOpen = $state(false);

  async function load() {
    const now = Date.now();
    const q7 = `from=${now - 7 * DAY}&to=${now}`;
    const [r, j, s, t, a] = await Promise.all([
      get<RepoSummary[]>("/repos"),
      get<Job[]>("/jobs?limit=8").catch(() => []),
      get<any>(`/stats/summary?${q7}`).catch(() => null),
      get<any>(`/stats/traffic?from=${now - 14 * DAY}&to=${now}&bucket=day`).catch(() => null),
      get<any>(`/stats/addons?${q7}&limit=5`).catch(() => null),
    ]);
    repos = r;
    recent = j;
    sum = s;
    traffic = t;
    addons = a?.addons ?? [];
  }
  onMount(() => {
    load();
    const tick = () => !document.hidden && get("/stats/live").then((l) => (live = l)).catch(() => {});
    tick();
    const t = setInterval(tick, 4000);
    return () => clearInterval(t);
  });
  let seen = session.jobsDone;
  $effect(() => {
    if (session.jobsDone !== seen) {
      seen = session.jobsDone;
      load();
    }
  });

  const c = $derived(sum?.current);
  const p = $derived(sum?.previous);
  const daily = $derived((sum?.daily ?? []) as any[]);
  const failed = $derived(recent.filter((j) => j.status === "failed"));
  const sys = $derived(session.system);
  const upd = $derived(sys?.update);
  // Decorative grid reference derived from the server URL (stable per server).
  const coords = $derived.by(() => {
    const u = sys?.public_url ?? "";
    let h = 0;
    for (const ch of u) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
    return `GRID ${String(h % 1000).padStart(3, "0")} ${String((h >>> 10) % 1000).padStart(3, "0")}`;
  });
  const tone = (s: string) => (s === "succeeded" ? "ok" : s === "failed" || s === "interrupted" ? "err" : s === "running" ? "accent" : "neutral");
  const repoSpark = (slug: string) => {
    const rows = ((traffic?.by_repo ?? []) as any[]).filter((r) => r.repo === slug);
    const by = new Map(rows.map((r) => [r.t, r.served]));
    return ((traffic?.series ?? []) as any[]).map((s) => by.get(s.t) ?? 0);
  };
  const speed = (v: number) => `${bytes(v)}/s`;
</script>

<div class="page">
  <header class="hero card bracket">
    <Topo opacity={0.14} coords={coords} />
    <div class="grow">
      <div class="eyebrow">ArmaXSync server{sys ? ` · v${sys.version}` : ""}</div>
      <h1>Dashboard</h1>
      <p class="muted small">{sys ? `${sys.public_url} · up since ${ago(sys.started)}` : ""}</p>
    </div>
    <div class="livebox">
      <div class="row tight"><span class="dot accent pulse"></span><span class="eyebrow">Live</span></div>
      <div class="lnum"><b>{speed(live?.throughput ?? 0)}</b><span class="xs muted">{live?.downloads ?? 0} downloads{live?.players != null ? ` · ${live.players} players` : ""}</span></div>
      <div class="lch">{#if (live?.series ?? []).length > 1}<Sparkline values={(live.series as [number, number][]).map((s) => s[1])} width={220} height={44} label="Live throughput, last 15 minutes" />{/if}</div>
    </div>
    {#if isAdmin()}<Button variant="primary" icon={Plus} onclick={() => (createOpen = true)}>New repository</Button>{/if}
  </header>

  {#if !repos}
    <div class="grid4"><Skeleton block height={110} /><Skeleton block height={110} /><Skeleton block height={110} /><Skeleton block height={110} /></div>
  {:else}
    <div class="grid4">
      <Stat label="Served · 7 days" icon={HardDriveDownload} value={c?.served ?? 0} format={bytes} spark={daily.map((d) => d.served)} delta={c ? change(c.served, p.served) : null} deltaLabel="vs previous 7 days" />
      {#if sum?.players_enabled !== false}
        <Stat label="Players · 7 days" icon={Users} value={c?.players ?? 0} spark={daily.map((d) => d.players)} delta={c ? change(c.players, p.players) : null} deltaLabel="vs previous 7 days" />
      {/if}
      <Stat label="Full downloads · 7 days" icon={Download} value={c?.files ?? 0} spark={daily.map((d) => d.downloads)} delta={c ? change(c.files, p.files) : null} deltaLabel="vs previous 7 days" />
      <Stat label="Server" icon={ServerCog} value={sys ? `v${sys.version}` : "–"} tone={upd?.available ? "accent" : undefined} hint={upd?.available ? `${upd.latest?.version} available` : `${repos.length} repositor${repos.length === 1 ? "y" : "ies"} · ${bytes(repos.reduce((s, r) => s + r.size, 0))}`} />
    </div>

    <div class="cols">
      <Section title="Repositories" icon={Database} flush>
        {#snippet actions()}<Button size="sm" variant="ghost" href="#/repos">All repositories</Button>{/snippet}
        {#if !repos.length}
          <EmptyState icon={Database} title="No repositories yet" description="Create one, then upload mods, push from the desktop app or mirror an existing ArmA3Sync repository." compact>
            {#if isAdmin()}<Button variant="primary" icon={Plus} onclick={() => (createOpen = true)}>New repository</Button>{/if}
          </EmptyState>
        {:else}
          <ul class="repos">
            {#each repos.slice(0, 8) as r, i (r.slug)}
              <li style:animation-delay="{i * 40}ms">
                <button onclick={() => navigate(`/repos/${encodeURIComponent(r.slug)}`)}>
                  <span class="grow" style="min-width:0">
                    <span class="strong ellipsis" style="display:block">{r.name}</span>
                    <span class="xs muted">rev {r.revision} · {bytes(r.size)} · {r.build_date ? `published ${ago(r.build_date)}` : "never published"}</span>
                  </span>
                  <span class="hide-sm" title="Served, last 14 days"><Sparkline values={repoSpark(r.slug)} width={90} height={26} /></span>
                  {#if r.visibility === "private"}<Badge plain icon={Lock}>Private</Badge>{:else}<Badge plain icon={Globe}>Public</Badge>{/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </Section>

      <div class="stack">
        <Section title="Top addons · 7 days" icon={Package} flush>
          {#snippet actions()}<Button size="sm" variant="ghost" icon={ChartColumn} href="#/stats?view=addons">Statistics</Button>{/snippet}
          {#if addons.length}
            <ul class="top">
              {#each addons as a, i}
                {@const max = addons[0].bytes || 1}
                <li>
                  <span class="rank">{i + 1}</span>
                  <span class="grow" style="min-width:0">
                    <span class="mono small ellipsis" style="display:block">{a.addon}</span>
                    <span class="meter"><span style:width="{(a.bytes / max) * 100}%"></span></span>
                  </span>
                  <span class="xs muted tabular nowrap">{bytes(a.bytes)}</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="small muted pad">No downloads in the last 7 days.</p>
          {/if}
        </Section>
        <Section title="Recent jobs" flush>
          {#snippet actions()}<Button size="sm" variant="ghost" href="#/jobs">All jobs</Button>{/snippet}
          {#if failed.length}<div class="pad"><span class="small err-text row tight"><CircleAlert size={14} />{failed.length} recent job{failed.length > 1 ? "s" : ""} failed</span></div>{/if}
          <ul class="jobs">
            {#each recent as j (j.id)}
              <li>
                <button onclick={() => openJob(j.id)}>
                  <Badge tone={tone(j.status)} dot pulse={j.status === "running"}>{j.status}</Badge>
                  <span class="grow ellipsis small">{jobLabel(j.kind)}{j.repo ? ` · ${j.repo}` : ""}</span>
                  <span class="xs subtle nowrap">{ago(j.created_at)}</span>
                </button>
              </li>
            {:else}
              <li class="pad small muted">No jobs yet.</li>
            {/each}
          </ul>
        </Section>
        {#if upd && !upd.available && isAdmin()}
          <p class="xs subtle row tight nowrap" title={upd.error ?? undefined}><ArrowUpCircle size={13} />ArmaXSync {upd.current} · {upd.error ? (/404/.test(upd.error) ? "no release published yet" : "update check failed") : upd.checked_at ? `up to date, checked ${ago(upd.checked_at)}` : "not checked yet"}</p>
        {/if}
      </div>
    </div>
  {/if}
</div>

<CreateRepo bind:open={createOpen} />

<style>
  .hero { position: relative; display: flex; align-items: center; gap: 20px; flex-wrap: wrap; padding: 22px 22px; background-color: var(--surface-1); overflow: hidden; }
  .hero > :global(:not(.topo)) { position: relative; z-index: 1; }
  .hero h1 { font-size: 30px; margin: 2px 0 4px; }
  .hero p { margin: 0; }
  .livebox { display: flex; flex-direction: column; gap: 4px; min-width: 220px; padding: 8px 12px; border-left: 1px solid var(--line); }
  .lnum { display: flex; flex-direction: column; }
  .lnum b { font-family: var(--font-display); font-size: 22px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .lch { height: 48px; }
  .grid4 { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; }
  .cols { display: grid; grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr); gap: 16px; align-items: start; }
  @media (max-width: 1100px) { .cols { grid-template-columns: 1fr; } }
  .repos, .jobs, .top { list-style: none; margin: 0; padding: 0; }
  .repos li { animation: rise 320ms var(--ease) both; }
  .repos li + li, .jobs li + li { border-top: 1px solid var(--line); }
  .repos button, .jobs button { display: flex; align-items: center; gap: 12px; width: 100%; padding: 11px 16px; background: none; border: 0; color: var(--fg); font: inherit; text-align: left; cursor: pointer; }
  .repos button:hover, .jobs button:hover { background: var(--surface-2); }
  .top li { display: flex; align-items: center; gap: 12px; padding: 8px 16px; }
  .rank { width: 18px; font-family: var(--font-display); font-weight: 700; color: var(--fg-3); }
  .meter { display: block; height: 4px; border-radius: 2px; background: var(--surface-3); margin-top: 4px; overflow: hidden; }
  .meter span { display: block; height: 100%; background: var(--accent); border-radius: 2px; }
  .pad { padding: 10px 16px; margin: 0; }
  @keyframes rise { from { opacity: 0; transform: translateY(6px); } }
  @media (max-width: 700px) { .livebox { border-left: 0; padding: 0; } }
</style>
