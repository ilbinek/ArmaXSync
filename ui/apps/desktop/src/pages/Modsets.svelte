<script lang="ts">
  import { onMount } from "svelte";
  import { toast } from "@axs/kit";
  import { call, type Addon, type GroupNode, type Profile } from "../api";
  import { app } from "../store.svelte";
  import GroupTree from "../components/GroupTree.svelte";

  let profile = $state<Profile | null>(null);
  let addons = $state<Addon[]>([]);
  let filter = $state("");
  let picked: Set<string> = $state(new Set());
  let target = $state("");
  let repoForModset = $state("");
  let repoEvents = $state<any[]>([]);
  let eventForModset = $state("");
  let saving: any;

  async function load() {
    profile = await call<Profile>("profile_get", { name: app.state!.active_profile });
    addons = await call<Addon[]>("addons", { profile });
    target = profile.groups.find((g) => g.type === "group" && !g.modset)?.name ?? "";
  }
  onMount(load);

  const available = $derived(new Set(addons.map((a) => a.name.toLowerCase())));
  const shown = $derived(addons.filter((a) => !filter || a.key.includes(filter.toLowerCase()) || a.display_name.toLowerCase().includes(filter.toLowerCase())));
  const groups = $derived(profile?.groups.filter((g) => g.type === "group" && !g.modset).map((g) => g.name) ?? []);

  function changed() {
    clearTimeout(saving);
    saving = setTimeout(save, 400);
  }
  async function save() {
    if (!profile) return;
    await call("profile_save", { profile, original: profile.name }).catch((e) => toast(String(e), "err"));
  }

  function addGroup() {
    const name = prompt("Group name", `Group ${(profile?.groups.length ?? 0) + 1}`);
    if (!name || !profile) return;
    if (profile.groups.some((g) => g.name === name)) return toast("A group with this name exists", "err");
    profile.groups.push({ type: "group", name, children: [], modset: null });
    target = name;
    changed();
  }

  function addPicked() {
    if (!profile || !target) return;
    const g = profile.groups.find((x) => x.type === "group" && x.name === target);
    if (!g || g.type !== "group") return;
    for (const name of picked) {
      if (!g.children.some((c) => c.name.toLowerCase() === name.toLowerCase())) g.children.push({ type: "addon", name, selected: true, optional: false });
    }
    picked = new Set();
    changed();
  }

  async function loadEvents() {
    repoEvents = [];
    eventForModset = "";
    if (!repoForModset) return;
    try {
      repoEvents = (await call("repo_info", { name: repoForModset })).events;
    } catch {
      /* offline */
    }
  }

  async function addModset() {
    try {
      profile = await call<Profile>("apply_modset", { profile: profile!.name, repo: repoForModset, event: eventForModset || null });
      toast("Modset added");
    } catch (e: any) {
      toast(String(e), "err");
    }
  }

  function move(i: number, d: number) {
    if (!profile) return;
    const p = profile.addon_priority;
    const j = i + d;
    if (j < 0 || j >= p.length) return;
    [p[i], p[j]] = [p[j], p[i]];
    changed();
  }
  function addToPriority(name: string) {
    if (profile && !profile.addon_priority.some((p) => p.toLowerCase() === name.toLowerCase())) {
      profile.addon_priority.push(name);
      changed();
    }
  }
</script>

{#if profile}
  <div class="row" style="margin-bottom:12px">
    <div><h1>Modsets</h1><div class="muted">Profile <b>{profile.name}</b> — choose which addons start with the game.</div></div>
  </div>
  <div class="cols">
    <div class="card stack">
      <div class="row"><h2 style="margin:0">Available addons <span class="muted small">({addons.length})</span></h2></div>
      <input placeholder="Search…" bind:value={filter} />
      <div class="list">
        {#each shown as a}
          <label class="check item">
            <input type="checkbox" checked={picked.has(a.name)} onchange={(e) => { const s = new Set(picked); (e.target as HTMLInputElement).checked ? s.add(a.name) : s.delete(a.name); picked = s; }} />
            <span>{a.name}{#if a.display_name !== a.name} <span class="muted small">{a.display_name}</span>{/if}</span>
            {#if a.source.kind === "workshop"}<span class="badge">workshop</span>{/if}
            <button class="btn ghost small" title="Add to load order" onclick={(e) => { e.preventDefault(); addToPriority(a.name); }}>↧</button>
          </label>
        {:else}<div class="empty small">No addons found. Add search folders in Settings or synchronize a repository.</div>{/each}
      </div>
      <div class="row">
        <select bind:value={target} style="flex:1"><option value="" disabled>Group…</option>{#each groups as g}<option>{g}</option>{/each}</select>
        <button class="btn" onclick={addGroup}>New group</button>
        <button class="btn primary" disabled={!picked.size || !target} onclick={addPicked}>Add {picked.size || ""} to group</button>
      </div>
    </div>

    <div class="stack">
      <div class="card stack">
        <h2 style="margin:0">Groups</h2>
        <div class="tree-root">
          {#if profile.groups.length}
            <GroupTree nodes={profile.groups} {available} onchange={changed} ondelete={(n: GroupNode) => { profile!.groups.splice(profile!.groups.indexOf(n), 1); changed(); }} />
          {:else}<div class="empty small">No groups yet. Create one, or add a repository modset below.</div>{/if}
        </div>
        <hr />
        <div class="row">
          <select bind:value={repoForModset} onchange={loadEvents} style="flex:1">
            <option value="">Repository modset…</option>
            {#each app.state?.repos ?? [] as r}<option value={r.name}>{r.name}</option>{/each}
          </select>
          {#if repoEvents.length}
            <select bind:value={eventForModset} style="flex:1"><option value="">whole repository</option>{#each repoEvents as e}<option value={e.name}>event {e.name}</option>{/each}</select>
          {/if}
          <button class="btn" disabled={!repoForModset} onclick={addModset}>Add / refresh</button>
        </div>
      </div>
      <div class="card stack">
        <h2 style="margin:0">Load order</h2>
        <div class="small muted">Addons listed here load first, in this order; the rest follow alphabetically.</div>
        {#each profile.addon_priority as p, i}
          <div class="row prio">
            <span>{i + 1}. {p}</span><div class="spacer"></div>
            <button class="btn ghost small" onclick={() => move(i, -1)}>↑</button>
            <button class="btn ghost small" onclick={() => move(i, 1)}>↓</button>
            <button class="btn ghost small" onclick={() => { profile!.addon_priority.splice(i, 1); changed(); }}>✕</button>
          </div>
        {:else}<div class="muted small">Empty (use ↧ next to an addon).</div>{/each}
      </div>
    </div>
  </div>
{:else}
  <div class="empty">Loading…</div>
{/if}

<style>
  .cols { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; align-items: start; }
  .list { max-height: 50vh; overflow: auto; border: 1px solid var(--line); border-radius: 7px; }
  .item { padding: 5px 8px; border-bottom: 1px solid var(--line); }
  .item:last-child { border-bottom: 0; }
  .item button { margin-left: auto; }
  .prio { padding: 2px 0; }
  @media (max-width: 980px) { .cols { grid-template-columns: 1fr; } }
</style>
