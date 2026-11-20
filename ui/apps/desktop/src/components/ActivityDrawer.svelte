<script lang="ts">
  import { Drawer, Progress, EmptyState, Sparkline, activity, bytes, speed, duration, ago, navigate } from "@axs/kit";
  import { Download, X, Bell, CircleCheck, CircleAlert, Info, TriangleAlert } from "@lucide/svelte";
  import { app, cancelSync } from "../store.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();
  const active = $derived(Object.values(app.progress));
  const icons = { ok: CircleCheck, err: CircleAlert, info: Info, warn: TriangleAlert };
</script>

<Drawer bind:open title="Activity" subtitle="Downloads and recent messages" width={480}>
  <div class="stack">
    <h3>Downloads</h3>
    {#each active as p (p.repo)}
      <div class="card raised stack tight">
        <div class="row nowrap">
          <Download size={16} />
          <button class="linkish grow ellipsis strong" onclick={() => { open = false; navigate(`/repos/${encodeURIComponent(p.repo)}`); }}>{p.repo}</button>
          <button class="btn ghost sm" onclick={() => cancelSync(p.repo)}><X size={14} />Cancel</button>
        </div>
        <Progress
          value={p.bytes_done}
          max={p.bytes_total}
          label={`${bytes(p.bytes_done)} of ${bytes(p.bytes_total)} · ${speed(p.speed)}${p.speed > 0 ? ` · ${duration((p.bytes_total - p.bytes_done) / p.speed)} left` : ""}`}
          detail={p.current}
        />
        <div class="row between">
          <span class="xs muted">{p.files_done} of {p.files_total} files{#if p.failed}<span class="err-text"> · {p.failed} failed</span>{/if}</span>
          {#if (app.speeds[p.repo] ?? []).length > 2}<Sparkline values={app.speeds[p.repo]} width={120} height={22} label="Download speed" />{/if}
        </div>
      </div>
    {:else}
      <p class="muted small">Nothing is downloading.</p>
    {/each}

    <h3 style="margin-top:8px">Recent messages</h3>
    {#if activity.length}
      <ul class="list">
        {#each activity as a (a.id + "-" + a.at)}
          {@const Icon = icons[a.kind]}
          <li class="act {a.kind}">
            <Icon size={15} />
            <div class="grow">
              <div class="small">{a.text}</div>
              {#if a.detail}<div class="xs muted pre">{a.detail}</div>{/if}
            </div>
            <span class="xs subtle nowrap">{ago(a.at)}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <EmptyState icon={Bell} title="No messages" description="Results and errors from this session appear here." compact />
    {/if}
  </div>
</Drawer>

<style>
  .linkish { background: none; border: 0; padding: 0; color: var(--fg); font: inherit; cursor: pointer; text-align: left; }
  .linkish:hover { color: var(--accent-text); }
  .card :global(svg) { color: var(--accent); }
  .act { align-items: flex-start; }
  .act > :global(svg) { margin-top: 2px; color: var(--fg-3); }
  .act.err > :global(svg) { color: var(--err); }
  .act.ok > :global(svg) { color: var(--ok); }
  .act.warn > :global(svg) { color: var(--warn); }
  .act.info > :global(svg) { color: var(--info); }
  .pre { white-space: pre-line; overflow-wrap: anywhere; }
</style>
