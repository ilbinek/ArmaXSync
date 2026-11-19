<script lang="ts">
  import { Section, Button, Field, PathInput, Switch, Progress, Alert, Stat, bytes, speed, duration, toast, toastError, confirmDialog } from "@axs/kit";
  import { Download, Eye, X, ChevronRight, FilePlus2, FilePen, FileX2, FolderX, CircleCheck } from "@lucide/svelte";
  import { call, pickFolder, type RepoConfig, type RepoInfo, type SyncOutcome, type PlanOp } from "../../api";
  import { app, refresh, startSync, cancelSync } from "../../store.svelte";

  let { repo, info }: { repo: RepoConfig; info: RepoInfo | null } = $props();

  let event = $state("");
  // svelte-ignore state_referenced_locally
  let dest = $state(repo.destination ?? "");
  // svelte-ignore state_referenced_locally
  let exact = $state(repo.exact_match);
  let noDelete = $state(false);
  let plan = $state<SyncOutcome | null>(null);
  let planning = $state(false);
  let filter = $state("");
  let expanded = $state<Record<string, boolean>>({});

  const progress = $derived(app.progress[repo.name]);
  const params = $derived({ event: event || null, exact, no_delete: noDelete });

  async function saveDest() {
    if (dest === (repo.destination ?? "")) return;
    try {
      await call("repo_save", { repo: { ...$state.snapshot(repo), destination: dest || null }, original: repo.name });
      await refresh();
      toast("Download folder updated");
    } catch (e) {
      toastError(e);
    }
  }

  async function preview() {
    planning = true;
    plan = null;
    try {
      plan = await call<SyncOutcome>("sync_plan", { name: repo.name, params });
    } catch (e) {
      toastError(e);
    } finally {
      planning = false;
    }
  }

  async function sync() {
    if (exact && !noDelete) {
      const ok = await confirmDialog({
        title: "Exact match is on",
        message: "Extra files and folders in the download folder that are not part of the repository will be deleted.",
        confirmLabel: "Synchronize",
        danger: true,
      });
      if (!ok) return;
    }
    plan = null;
    try {
      await startSync(repo.name, params);
    } catch (e) {
      toastError(e);
    }
  }

  type Group = { name: string; fetch: number; update: number; del: number; bytes: number; ops: PlanOp[] };
  const groups = $derived.by(() => {
    if (!plan) return [] as Group[];
    const m = new Map<string, Group>();
    for (const o of plan.plan.ops) {
      if (o.op === "mkdir") continue;
      const top = o.path.split("/")[0];
      const g = m.get(top) ?? { name: top, fetch: 0, update: 0, del: 0, bytes: 0, ops: [] };
      if (o.op === "fetch") {
        if (o.local_size != null) g.update++;
        else g.fetch++;
        g.bytes += o.size;
      } else g.del++;
      g.ops.push(o);
      m.set(top, g);
    }
    return [...m.values()].sort((a, b) => b.bytes - a.bytes);
  });
  const shown = $derived(groups.filter((g) => g.name.toLowerCase().includes(filter.toLowerCase()) || g.ops.some((o) => o.path.toLowerCase().includes(filter.toLowerCase()))));
  const counts = $derived(groups.reduce((a, g) => ({ n: a.n + g.fetch, u: a.u + g.update, d: a.d + g.del }), { n: 0, u: 0, d: 0 }));
</script>

<div class="stack loose">
  <Section title="Synchronize" icon={Download}>
    {#if progress}
      <div class="stack">
        <Progress
          value={progress.bytes_done}
          max={progress.bytes_total}
          label={`${progress.files_done} of ${progress.files_total} files · ${bytes(progress.bytes_done)} of ${bytes(progress.bytes_total)}`}
          detail={progress.current}
        />
        <div class="row">
          <span class="small muted">{speed(progress.speed)}{progress.speed > 0 ? ` · ${duration((progress.bytes_total - progress.bytes_done) / progress.speed)} left` : ""}</span>
          {#if progress.failed}<span class="small err-text">{progress.failed} file(s) failed</span>{/if}
          <div class="spacer"></div>
          <Button icon={X} onclick={() => cancelSync(repo.name)}>Cancel</Button>
        </div>
      </div>
    {:else}
      <div class="form-grid">
        <Field label="What to download" hint={event ? info?.events.find((e) => e.name === event)?.description ?? "" : "Every addon in the repository."}>
          {#snippet children(id)}
            <select {id} bind:value={event} onchange={() => (plan = null)}>
              <option value="">Whole repository</option>
              {#each info?.events ?? [] as e}<option value={e.name}>Event: {e.name} ({e.addons.length} addons)</option>{/each}
            </select>
          {/snippet}
        </Field>
        <Field label="Download folder" required>
          {#snippet children(id)}<PathInput {id} bind:value={dest} browse={() => pickFolder("Download folder")} onchange={saveDest} />{/snippet}
        </Field>
      </div>
      <div class="row loose">
        <Switch bind:checked={exact} onchange={() => (plan = null)} label="Exact match" hint="Also remove extra files in the download folder root." />
        <Switch bind:checked={noDelete} onchange={() => (plan = null)} label="Keep removed files" hint="Download changes but delete nothing." />
      </div>
      {#if !repo.destination}<Alert tone="warn">Choose a download folder first.</Alert>{/if}
    {/if}
    {#snippet footer()}
      {#if !progress}
        <Button icon={Eye} loading={planning} disabled={!repo.destination} onclick={preview}>Preview changes</Button>
        <Button variant="primary" icon={Download} disabled={!repo.destination} onclick={sync}>Synchronize</Button>
      {/if}
    {/snippet}
  </Section>

  {#if plan}
    {#if !groups.length}
      <Alert tone="ok" icon={CircleCheck} title="Everything is up to date">Revision {plan.revision} is already in {plan.destination}.</Alert>
    {:else}
      <div class="grid3">
        <Stat label="To download" value={bytes(plan.plan.fetch_bytes)} hint={`${plan.plan.fetch_count} files`} tone="accent" />
        <Stat label="New / changed" value={`${counts.n} / ${counts.u}`} hint="files" />
        <Stat label="To delete" value={counts.d} tone={counts.d ? "warn" : undefined} hint="files and folders" />
      </div>
      <Section title={`Changes by addon (${groups.length})`} flush>
        {#snippet actions()}<input type="search" placeholder="Filter…" bind:value={filter} style="width:200px" />{/snippet}
        <ul class="plan">
          {#each shown as g (g.name)}
            <li>
              <button class="grp" onclick={() => (expanded[g.name] = !expanded[g.name])} aria-expanded={!!expanded[g.name]}>
                <ChevronRight size={14} style={expanded[g.name] ? "transform:rotate(90deg)" : ""} />
                <span class="mono strong grow ellipsis">{g.name}</span>
                {#if g.fetch}<span class="tag new"><FilePlus2 size={12} />{g.fetch}</span>{/if}
                {#if g.update}<span class="tag upd"><FilePen size={12} />{g.update}</span>{/if}
                {#if g.del}<span class="tag del"><FileX2 size={12} />{g.del}</span>{/if}
                <span class="sz tabular small">{g.bytes ? bytes(g.bytes) : ""}</span>
              </button>
              {#if expanded[g.name]}
                <ul class="files">
                  {#each g.ops as o}
                    <li class={o.op === "delete" ? "del" : o.op === "fetch" && o.local_size != null ? "upd" : "new"}>
                      {#if o.op === "delete"}{#if o.dir}<FolderX size={13} />{:else}<FileX2 size={13} />{/if}{:else if o.op === "fetch" && o.local_size != null}<FilePen size={13} />{:else}<FilePlus2 size={13} />{/if}
                      <span class="mono grow ellipsis" title={o.path}>{o.path}</span>
                      {#if o.op === "fetch"}<span class="tabular xs muted">{bytes(o.size)}{o.zsync && o.local_size != null ? " · delta" : ""}</span>{/if}
                    </li>
                  {/each}
                </ul>
              {/if}
            </li>
          {/each}
        </ul>
      </Section>
    {/if}
  {/if}
</div>

<style>
  .plan, .files { list-style: none; margin: 0; padding: 0; }
  .plan > li { border-bottom: 1px solid var(--line); }
  .plan > li:last-child { border-bottom: 0; }
  .grp { display: flex; align-items: center; gap: 10px; width: 100%; padding: 9px 16px; background: none; border: 0; color: var(--fg); font: inherit; cursor: pointer; text-align: left; }
  .grp:hover { background: var(--surface-2); }
  .tag { display: inline-flex; align-items: center; gap: 3px; font-size: 12px; font-weight: 600; padding: 1px 6px; border-radius: var(--r-sm); }
  .tag.new { color: var(--ok); background: var(--ok-soft); }
  .tag.upd { color: var(--info); background: var(--info-soft); }
  .tag.del { color: var(--err); background: var(--err-soft); }
  .sz { width: 80px; text-align: right; color: var(--fg-2); }
  .files { padding: 0 16px 10px 40px; }
  .files li { display: flex; align-items: center; gap: 8px; padding: 3px 0; font-size: 12.5px; }
  .files li.new :global(svg) { color: var(--ok); }
  .files li.upd :global(svg) { color: var(--info); }
  .files li.del :global(svg) { color: var(--err); }
  .files li.del .mono { text-decoration: line-through; color: var(--fg-2); }
</style>
