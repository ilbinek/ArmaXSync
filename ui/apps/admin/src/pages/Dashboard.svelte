<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Stat, Section, Button, Badge, BarChart, EmptyState, Skeleton, bytes, ago, navigate } from "@axs/kit";
  import { Plus, Database, Activity, ArrowUpCircle, CircleAlert, Lock, Globe, ServerCog } from "@lucide/svelte";
  import { get, jobLabel, type RepoSummary, type Job } from "../api";
  import { session, isAdmin, openJob } from "../session.svelte";
  import CreateRepo from "../components/CreateRepo.svelte";

  let repos = $state<RepoSummary[] | null>(null);
  let recent = $state<Job[]>([]);
  let daily = $state<{ day: string; bytes: number }[]>([]);
  let createOpen = $state(false);

  async function load() {
    const [r, j, s] = await Promise.all([get<RepoSummary[]>("/repos"), get<Job[]>("/jobs?limit=8").catch(() => []), get<any>("/stats").catch(() => null)]);
    repos = r;
    recent = j;
    if (s) {
      const m = new Map<string, number>();
      for (const d of s.daily) m.set(d.day, (m.get(d.day) ?? 0) + d.bytes);
      daily = Array.from({ length: 14 }, (_, i) => {
        const day = new Date(Date.now() - (13 - i) * 86400000).toISOString().slice(0, 10);
        return { day, bytes: m.get(day) ?? 0 };
      });
    }
  }
  onMount(load);
  $effect(() => {
    session.jobsDone;
    load();
  });

  const total = $derived((repos ?? []).reduce((a, r) => ({ size: a.size + r.size, files: a.files + r.files, served: a.served + r.bytes_served }), { size: 0, files: 0, served: 0 }));
  const failed = $derived(recent.filter((j) => j.status === "failed"));
  const sys = $derived(session.system);
  const upd = $derived(sys?.update);
  const tone = (s: string) => (s === "succeeded" ? "ok" : s === "failed" || s === "interrupted" ? "err" : s === "running" ? "accent" : "neutral");
</script>

<div class="page">
  <PageHeader title="Dashboard" subtitle={sys ? `${sys.public_url} · up since ${ago(sys.started)}` : undefined}>
    {#snippet actions()}{#if isAdmin()}<Button variant="primary" icon={Plus} onclick={() => (createOpen = true)}>New repository</Button>{/if}{/snippet}
  </PageHeader>

  {#if !repos}
    <div class="grid3"><Skeleton block height={96} /><Skeleton block height={96} /><Skeleton block height={96} /><Skeleton block height={96} /></div>
  {:else}
    <div class="grid3">
      <Stat label="Repositories" value={repos.length} icon={Database} hint={`${total.files.toLocaleString()} files`} />
      <Stat label="Hosted" value={bytes(total.size)} hint="published content" />
      <Stat label="Served" value={bytes(total.served)} hint="since the server started" />
      <Stat label="Server" value={sys ? `v${sys.version}` : "–"} icon={ServerCog} tone={upd?.available ? "accent" : undefined} hint={upd?.available ? `${upd.latest?.version} available` : upd?.checked_at ? `checked ${ago(upd.checked_at)}` : undefined} />
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
            {#each repos.slice(0, 8) as r (r.slug)}
              <li>
                <button onclick={() => navigate(`/repos/${encodeURIComponent(r.slug)}`)}>
                  <span class="grow" style="min-width:0">
                    <span class="strong ellipsis" style="display:block">{r.name}</span>
                    <span class="xs muted">rev {r.revision} · {bytes(r.size)} · {r.build_date ? `published ${ago(r.build_date)}` : "never published"}</span>
                  </span>
                  {#if r.has_mirror}<Badge plain>Mirror</Badge>{/if}
                  {#if r.visibility === "private"}<Badge plain icon={Lock}>Private</Badge>{:else}<Badge plain icon={Globe}>Public</Badge>{/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </Section>

      <div class="stack">
        <Section title="Downloads" icon={Activity} description="Bytes served per day, last 14 days.">
          {#if daily.length}<BarChart data={daily.map((d) => ({ label: d.day.slice(5), value: d.bytes }))} format={bytes} height={150} label="Served per day" />{/if}
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
  .cols { display: grid; grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr); gap: 16px; align-items: start; }
  @media (max-width: 1100px) { .cols { grid-template-columns: 1fr; } }
  .repos, .jobs { list-style: none; margin: 0; padding: 0; }
  .repos li + li, .jobs li + li { border-top: 1px solid var(--line); }
  .repos button, .jobs button { display: flex; align-items: center; gap: 10px; width: 100%; padding: 11px 16px; background: none; border: 0; color: var(--fg); font: inherit; text-align: left; cursor: pointer; }
  .repos button:hover, .jobs button:hover { background: var(--surface-2); }
  .pad { padding: 10px 16px; }
</style>
