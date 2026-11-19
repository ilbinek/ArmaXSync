<script lang="ts">
  import { onMount } from "svelte";
  import cronstrue from "cronstrue";
  import { Section, Field, Switch, TagListInput, PasswordInput, SaveBar, Button, Menu, Alert, EmptyState, useUnsavedGuard, snapshot, toast, toastError, errorText, date, ago } from "@axs/kit";
  import { FolderSync, Timer, Plus, X, Download, Clock } from "@lucide/svelte";
  import { get, put, patch, post, type RepoDetail, type MirrorSettings } from "../../api";
  import { openJob } from "../../session.svelte";

  let { repo, onsaved }: { repo: RepoDetail; onsaved: () => void } = $props();

  type Schedule = { id?: number; kind: "build" | "mirror"; cron: string; enabled: boolean; last_run?: number | null; next_run?: number | null };
  const blank: MirrorSettings = { url: "", login: null, password_sealed: null, import_events: true, import_hidden: true, import_servers: false, include: [] };
  // svelte-ignore state_referenced_locally
  let mirror = $state<MirrorSettings>(structuredClone($state.snapshot(repo.settings.mirror) ?? blank));
  // svelte-ignore state_referenced_locally
  let savedMirror = $state(snapshot(repo.settings.mirror ?? blank));
  let password = $state("");
  let schedules = $state<Schedule[]>([]);
  let savedSchedules = $state("[]");
  let saving = $state(false);
  let error = $state<string | null>(null);

  async function loadSchedules() {
    try {
      schedules = await get<Schedule[]>(`/repos/${repo.slug}/schedules`);
      savedSchedules = snapshot(schedules.map(strip));
    } catch (e) {
      toastError(e);
    }
  }
  onMount(loadSchedules);

  const strip = (s: Schedule) => ({ kind: s.kind, cron: s.cron.trim(), enabled: s.enabled });
  const mirrorDirty = $derived(snapshot(mirror) !== savedMirror || password !== "");
  const schedDirty = $derived(snapshot(schedules.map(strip)) !== savedSchedules);
  const dirty = $derived(mirrorDirty || schedDirty);
  useUnsavedGuard(() => dirty);

  function describe(cron: string): { text: string; ok: boolean } {
    try {
      return { text: cronstrue.toString(cron.trim(), { use24HourTimeFormat: true, verbose: false }), ok: true };
    } catch {
      return { text: "Not a valid cron expression", ok: false };
    }
  }
  const invalid = $derived(schedules.some((s) => !describe(s.cron).ok));
  const urlBad = $derived(!!mirror.url.trim() && !/^(https?|ftp):\/\//i.test(mirror.url.trim()));

  async function save() {
    if (invalid || urlBad) return;
    saving = true;
    error = null;
    try {
      if (mirrorDirty && repo.mode === "managed") {
        await patch(`/repos/${repo.slug}`, {
          settings: { mirror: mirror.url.trim() ? { ...$state.snapshot(mirror), url: mirror.url.trim() } : null },
          mirror_password: password || undefined,
          rebuild: false,
        });
        savedMirror = snapshot(mirror);
        password = "";
      }
      if (schedDirty) {
        await put(`/repos/${repo.slug}/schedules`, schedules.map(strip));
        await loadSchedules();
      }
      toast("Automation saved");
      onsaved();
    } catch (e) {
      error = errorText(e);
    } finally {
      saving = false;
    }
  }

  async function pullNow() {
    try {
      const r = await post<{ job: number }>(`/repos/${repo.slug}/mirror`);
      openJob(r.job);
    } catch (e) {
      toastError(e);
    }
  }

  const presets = [
    ["*/30 * * * *", "Every 30 minutes"],
    ["0 * * * *", "Every hour"],
    ["0 */6 * * *", "Every 6 hours"],
    ["0 4 * * *", "Daily at 04:00"],
    ["0 4 * * 1", "Mondays at 04:00"],
  ];
</script>

<div class="stack loose narrowish">
  {#if repo.mode === "managed"}
    <Section title="Mirror another repository" icon={FolderSync} description="Pull an ArmA3Sync or ArmaXSync repository (HTTP or FTP) into this one. Changes download as deltas and publish as this repository's own revisions.">
      {#snippet actions()}{#if repo.has_mirror}<Button size="sm" icon={Download} disabled={mirrorDirty} tip={mirrorDirty ? "Save first" : undefined} onclick={pullNow}>Pull now</Button>{/if}{/snippet}
      <Field label="Source" hint="Auto-config URL (…/.a3s/autoconfig) or repository URL. Empty = no mirror." error={urlBad ? "Use an http://, https:// or ftp:// URL" : null}>
        {#snippet children(id)}<input {id} class="mono" bind:value={mirror.url} placeholder="https://example.com/arma3/.a3s/autoconfig" />{/snippet}
      </Field>
      {#if mirror.url.trim()}
        <div class="form-grid">
          <Field label="Login">{#snippet children(id)}<input {id} bind:value={mirror.login} placeholder="anonymous" />{/snippet}</Field>
          <Field label="Password" hint={mirror.password_sealed ? "Stored. Type to replace it." : undefined}>
            {#snippet children(id)}<PasswordInput {id} bind:value={password} placeholder={mirror.password_sealed ? "••••••••" : ""} />{/snippet}
          </Field>
        </div>
        <Field label="Only these folders" hint="Empty = everything. Press Enter after each, e.g. @ace.">
          {#snippet children(id)}<TagListInput {id} bind:values={mirror.include} placeholder="@ace" />{/snippet}
        </Field>
        <div class="stack tight">
          <Switch bind:checked={mirror.import_events} label="Import events" />
          <Switch bind:checked={mirror.import_hidden} label="Import protected folders" />
          <Switch bind:checked={mirror.import_servers} label="Import game servers" />
        </div>
      {/if}
    </Section>
  {/if}

  <Section title="Schedules" icon={Timer} description="Run jobs automatically. Times use the server's time zone.">
    {#snippet actions()}
      <Menu
        text="Add schedule"
        icon={Plus}
        items={[
          { label: "Rebuild", icon: Clock, onclick: () => schedules.push({ kind: "build", cron: "0 4 * * *", enabled: true }) },
          ...(repo.has_mirror || mirror.url.trim() ? [{ label: "Mirror pull", icon: FolderSync, onclick: () => schedules.push({ kind: "mirror" as const, cron: "0 */6 * * *", enabled: true }) }] : []),
        ]}
      />
    {/snippet}
    {#each schedules as s, i}
      {@const d = describe(s.cron)}
      <div class="sch card raised">
        <div class="row">
          <select bind:value={s.kind} style="width:auto">
            <option value="build">Rebuild</option>
            <option value="mirror">Mirror pull</option>
          </select>
          <input class="mono cron" bind:value={s.cron} aria-label="Cron expression" aria-invalid={!d.ok} />
          <Menu text="Presets" items={presets.map(([c, l]) => ({ label: l, hint: c, onclick: () => (s.cron = c) }))} />
          <div class="spacer"></div>
          <Switch bind:checked={s.enabled} label="On" />
          <button class="btn ghost icon sm" aria-label="Remove schedule" onclick={() => schedules.splice(i, 1)}><X size={15} /></button>
        </div>
        <div class="xs" class:err-text={!d.ok} class:muted={d.ok}>
          {d.text}{#if d.ok && s.next_run && s.enabled} · next {date(s.next_run)}{/if}{#if s.last_run} · last ran {ago(s.last_run)}{/if}
        </div>
      </div>
    {:else}
      <EmptyState icon={Timer} title="No schedules" description="E.g. pull from the mirror every 6 hours, or rebuild an external folder nightly." compact />
    {/each}
  </Section>

  {#if invalid}<Alert tone="warn">Fix the invalid cron expressions before saving.</Alert>{/if}
  <SaveBar {dirty} {saving} {error} onsave={save} ondiscard={() => ((mirror = JSON.parse(savedMirror)), (password = ""), loadSchedules())} />
</div>

<style>
  .narrowish { max-width: 900px; }
  .sch { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; }
  .cron { width: 170px; }
</style>
