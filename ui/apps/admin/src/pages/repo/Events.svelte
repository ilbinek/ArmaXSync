<script lang="ts">
  import { onMount } from "svelte";
  import { Button, Field, SegmentedControl, SaveBar, EmptyState, Skeleton, Alert, useUnsavedGuard, snapshot, confirmDialog, errorText, toast } from "@axs/kit";
  import { CalendarDays, Plus, Trash2, Search } from "@lucide/svelte";
  import { get, put, type EventDef } from "../../api";
  import { openJob } from "../../session.svelte";

  let { slug }: { slug: string } = $props();

  type Item = { name: string; optional: boolean };
  let events = $state<EventDef[] | null>(null);
  let saved = $state("");
  let addons = $state<string[]>([]);
  let userconfig = $state<string[]>([]);
  let sel = $state(0);
  let q = $state("");
  let only = $state<"all" | "in">("all");
  let saving = $state(false);
  let error = $state<string | null>(null);
  let loadError = $state<string | null>(null);

  async function load() {
    try {
      const r = await get<{ events: EventDef[]; addons: string[]; userconfig: string[] }>(`/repos/${slug}/events`);
      events = r.events;
      saved = snapshot(r.events);
      addons = r.addons;
      userconfig = r.userconfig;
    } catch (e) {
      loadError = errorText(e);
    }
  }
  onMount(load);

  const dirty = $derived(!!events && snapshot(events) !== saved);
  useUnsavedGuard(() => dirty);
  const cur = $derived(events?.[sel]);
  const nameErr = $derived(cur ? (!cur.name.trim() ? "Required" : events!.some((e, i) => i !== sel && e.name.trim().toLowerCase() === cur.name.trim().toLowerCase()) ? "Another event has this name" : null) : null);

  function add() {
    if (!events) return;
    let n = 1;
    while (events.some((e) => e.name === `Event ${n}`)) n++;
    events.push({ name: `Event ${n}`, description: "", addons: [], userconfig: [] });
    sel = events.length - 1;
  }
  async function remove() {
    if (!events || !cur) return;
    if (!(await confirmDialog({ title: `Delete event ${cur.name}?`, message: "Takes effect when you save.", confirmLabel: "Delete", danger: true }))) return;
    events.splice(sel, 1);
    sel = Math.max(0, sel - 1);
  }

  const stateOf = (list: Item[], name: string) => {
    const it = list.find((i) => i.name === name);
    return !it ? "off" : it.optional ? "opt" : "req";
  };
  function setState(list: Item[], name: string, v: "off" | "req" | "opt") {
    const i = list.findIndex((x) => x.name === name);
    if (v === "off") {
      if (i >= 0) list.splice(i, 1);
    } else if (i < 0) list.push({ name, optional: v === "opt" });
    else list[i].optional = v === "opt";
  }
  function setAll(list: Item[], names: string[], v: "off" | "req") {
    for (const n of names) setState(list, n, v);
  }
  const shownAddons = $derived(addons.filter((a) => a.toLowerCase().includes(q.toLowerCase()) && (only === "all" || (cur && stateOf(cur.addons, a) !== "off"))));

  async function save() {
    if (!events || nameErr) return;
    saving = true;
    error = null;
    try {
      const clean = events.map((e) => ({ ...e, name: e.name.trim() }));
      const r = await put<{ job: number | null }>(`/repos/${slug}/events`, clean);
      saved = snapshot(events);
      toast("Events saved", "ok", { detail: "Publishing new metadata for players." });
      if (r.job) openJob(r.job);
    } catch (e) {
      error = errorText(e);
    } finally {
      saving = false;
    }
  }
  const opts = [
    { value: "off" as const, label: "–" },
    { value: "req" as const, label: "Required" },
    { value: "opt" as const, label: "Optional" },
  ];
</script>

{#if loadError}
  <Alert tone="err" title="Could not load events">{loadError}</Alert>
{:else if !events}
  <div class="card"><Skeleton lines={6} /></div>
{:else}
  <div class="ev">
    <aside class="card flush list">
      <div class="lh"><strong class="grow">Events</strong><Button size="sm" icon={Plus} onclick={add}>New</Button></div>
      {#each events as e, i}
        <button class:active={i === sel} onclick={() => (sel = i)}>
          <span class="grow ellipsis">{e.name || "Unnamed"}</span>
          <span class="xs subtle">{e.addons.length}</span>
        </button>
      {:else}
        <p class="small muted pad">No events yet.</p>
      {/each}
    </aside>

    {#if cur}
      <section class="card stack">
        <div class="form-grid">
          <Field label="Name" required error={nameErr}>{#snippet children(id)}<input {id} bind:value={cur.name} />{/snippet}</Field>
          <Field label="Description" hint="Shown to players.">{#snippet children(id)}<input {id} bind:value={cur.description} placeholder="Sundays 19:00 CET" />{/snippet}</Field>
        </div>
        <div class="row between">
          <h3 style="margin:0">Addons <span class="subtle">· {cur.addons.filter((a) => !a.optional).length} required, {cur.addons.filter((a) => a.optional).length} optional</span></h3>
          <div class="row tight">
            <Button size="sm" variant="ghost" onclick={() => setAll(cur.addons, shownAddons, "req")}>Require shown</Button>
            <Button size="sm" variant="ghost" onclick={() => setAll(cur.addons, shownAddons, "off")}>Clear shown</Button>
          </div>
        </div>
        <div class="row">
          <div class="search grow"><Search size={14} /><input type="search" bind:value={q} placeholder="Filter addons…" /></div>
          <SegmentedControl size="sm" bind:value={only} options={[{ value: "all", label: "All" }, { value: "in", label: "In this event" }]} />
        </div>
        <ul class="picker">
          {#each shownAddons as a (a)}
            {@const st = stateOf(cur.addons, a)}
            <li class:on={st !== "off"}>
              <span class="mono small grow ellipsis">{a}</span>
              <SegmentedControl size="sm" value={st} options={opts} onchange={(v) => setState(cur.addons, a, v)} />
            </li>
          {:else}
            <li class="muted small">{addons.length ? "No matches." : "This repository has no addons yet."}</li>
          {/each}
        </ul>
        {#if userconfig.length}
          <h3 style="margin:6px 0 0">Userconfig</h3>
          <ul class="picker short">
            {#each userconfig as u (u)}
              <li class:on={stateOf(cur.userconfig, u) !== "off"}>
                <span class="mono small grow ellipsis">{u}</span>
                <SegmentedControl size="sm" value={stateOf(cur.userconfig, u)} options={opts} onchange={(v) => setState(cur.userconfig, u, v)} />
              </li>
            {/each}
          </ul>
        {/if}
        <div class="row"><Button variant="danger" size="sm" icon={Trash2} onclick={remove}>Delete event</Button></div>
      </section>
    {:else}
      <div class="card">
        <EmptyState icon={CalendarDays} title="No events" description="Events are named addon selections (an operation, a training night). Players download and load just that set.">
          <Button variant="primary" icon={Plus} onclick={add}>New event</Button>
        </EmptyState>
      </div>
    {/if}
  </div>
  <SaveBar {dirty} {saving} {error} saveLabel="Save & publish" message={nameErr ? "Fix the event name" : "Unsaved event changes"} onsave={save} ondiscard={() => ((events = JSON.parse(saved)), (sel = 0))} />
{/if}

<style>
  .ev { display: grid; grid-template-columns: 240px minmax(0, 1fr); gap: 14px; align-items: start; }
  @media (max-width: 800px) { .ev { grid-template-columns: 1fr; } }
  .list { display: flex; flex-direction: column; }
  .lh { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-bottom: 1px solid var(--line); }
  .list > button { display: flex; align-items: center; gap: 8px; padding: 9px 12px; background: none; border: 0; border-left: 2px solid transparent; color: var(--fg-2); font: inherit; text-align: left; cursor: pointer; }
  .list > button:hover { background: var(--surface-2); color: var(--fg); }
  .list > button.active { border-left-color: var(--accent); background: var(--surface-2); color: var(--fg); font-weight: 600; }
  .pad { padding: 12px; }
  .search { position: relative; min-width: 200px; }
  .search :global(svg) { position: absolute; left: 9px; top: 50%; transform: translateY(-50%); color: var(--fg-3); }
  .search input { padding-left: 30px; }
  .picker { list-style: none; margin: 0; padding: 0; max-height: 420px; overflow: auto; border: 1px solid var(--line); border-radius: var(--r); }
  .picker.short { max-height: 200px; }
  .picker li { display: flex; align-items: center; gap: 10px; padding: 5px 10px; border-bottom: 1px solid var(--line); }
  .picker li:last-child { border-bottom: 0; }
  .picker li.on { background: var(--accent-soft); }
</style>
