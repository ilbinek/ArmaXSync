<script lang="ts">
  import { onMount } from "svelte";
  import { toast } from "@axs/kit";
  import { get, put } from "../../api";

  type Item = { name: string; optional: boolean };
  type Ev = { name: string; description?: string | null; addons: Item[]; userconfig: Item[] };
  let { slug, onjob }: { slug: string; onjob: (id: number) => void } = $props();
  let events = $state<Ev[]>([]);
  let addons = $state<string[]>([]);
  let userconfig = $state<string[]>([]);
  let sel = $state(0);
  let dirty = $state(false);

  onMount(async () => {
    const r = await get(`/repos/${slug}/events`);
    events = r.events;
    addons = r.addons;
    userconfig = r.userconfig;
  });

  const cur = $derived(events[sel]);

  function add() {
    let n = 1;
    while (events.some((e) => e.name === `Event ${n}`)) n++;
    events.push({ name: `Event ${n}`, description: "", addons: [], userconfig: [] });
    sel = events.length - 1;
    dirty = true;
  }
  function remove() {
    if (!confirm(`Delete event ${cur.name}?`)) return;
    events.splice(sel, 1);
    sel = 0;
    dirty = true;
  }
  function itemState(list: Item[], name: string): "off" | "req" | "opt" {
    const it = list.find((i) => i.name === name);
    return !it ? "off" : it.optional ? "opt" : "req";
  }
  function cycle(list: Item[], name: string) {
    const i = list.findIndex((x) => x.name === name);
    if (i < 0) list.push({ name, optional: false });
    else if (!list[i].optional) list[i].optional = true;
    else list.splice(i, 1);
    dirty = true;
  }
  async function save() {
    try {
      const r = await put(`/repos/${slug}/events`, events);
      dirty = false;
      toast("Events saved; publishing metadata");
      onjob(r.job);
    } catch (e: any) {
      toast(e.message, "err");
    }
  }
  const label = { off: "—", req: "required", opt: "optional" };
</script>

<div class="layout">
  <div class="card flush">
    <div class="row" style="padding:10px"><h2 style="margin:0">Events</h2><div class="spacer"></div><button class="btn small" onclick={add}>New</button></div>
    {#each events as e, i}
      <button class="ev" class:active={i === sel} onclick={() => (sel = i)}>{e.name}<span class="muted small">{e.addons.length} addons</span></button>
    {:else}
      <div class="empty small">No events. Events let players download and launch a subset of the repository.</div>
    {/each}
  </div>
  <div class="card stack">
    {#if cur}
      <div class="grid2">
        <label class="field">Name <input bind:value={cur.name} oninput={() => (dirty = true)} /></label>
        <label class="field">Description <input bind:value={cur.description} oninput={() => (dirty = true)} /></label>
      </div>
      <h3>Addons <span class="muted small">(click to cycle: — / required / optional)</span></h3>
      <div class="chips">
        {#each addons as a}
          {@const s = itemState(cur.addons, a)}
          <button class="chip {s}" onclick={() => cycle(cur.addons, a)} title={label[s]}>{a}{#if s === "opt"} <i>optional</i>{/if}</button>
        {:else}<span class="muted">No addon folders in this repository yet.</span>{/each}
      </div>
      {#if userconfig.length}
        <h3>userconfig folders</h3>
        <div class="chips">
          {#each userconfig as u}
            {@const s = itemState(cur.userconfig, u)}
            <button class="chip {s}" onclick={() => cycle(cur.userconfig, u)}>{u}{#if s === "opt"} <i>optional</i>{/if}</button>
          {/each}
        </div>
      {/if}
      <div class="row"><button class="btn danger small" onclick={remove}>Delete event</button></div>
    {:else}
      <div class="empty">Select or create an event.</div>
    {/if}
    <div class="row end"><button class="btn primary" disabled={!dirty} onclick={save}>Save & publish</button></div>
  </div>
</div>

<style>
  .layout { display: grid; grid-template-columns: 240px 1fr; gap: 12px; align-items: start; }
  .ev { display: flex; flex-direction: column; align-items: flex-start; width: 100%; text-align: left; background: none; border: 0; border-top: 1px solid var(--line); padding: 8px 12px; cursor: pointer; font: inherit; color: var(--fg); }
  .ev.active { background: var(--accent-soft); }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { border: 1px solid var(--line); background: var(--panel); border-radius: 999px; padding: 3px 10px; cursor: pointer; font: inherit; color: var(--muted); }
  .chip.req { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .chip.opt { background: var(--accent-soft); color: var(--accent); border-color: var(--accent); }
  .chip i { font-size: 11px; }
  @media (max-width: 760px) { .layout { grid-template-columns: 1fr; } }
</style>
