<script lang="ts">
  import { onMount } from "svelte";
  import { PageHeader, Button, Menu, SegmentedControl, EmptyState, Skeleton, Badge, toast, toastError, promptDialog, confirmDialog } from "@axs/kit";
  import { Layers, Plus, Database, CalendarDays, FolderPlus, Search, Package, ArrowUpToLine, GripVertical, X, ChevronUp, ChevronDown, Check, LoaderCircle, ListOrdered, FolderOpen } from "@lucide/svelte";
  import { call, type Addon, type GroupNode, type Profile, type RepoInfo } from "../api";
  import { app, saveProfile, loadProfile } from "../store.svelte";
  import GroupTree from "../components/GroupTree.svelte";

  let profile = $state<Profile | null>(null);
  let addons = $state<Addon[] | null>(null);
  let filter = $state("");
  let source = $state<"all" | "local" | "workshop" | "unused">("all");
  let picked = $state<Set<string>>(new Set());
  let saveState = $state<"saved" | "saving" | "error">("saved");
  let timer: ReturnType<typeof setTimeout>;
  let events = $state<Record<string, RepoInfo["events"] | "error">>({});

  async function load() {
    await loadProfile();
    profile = structuredClone($state.snapshot(app.profile)) as Profile;
    addons = await call<Addon[]>("addons", { profile }).catch((e) => (toastError(e), []));
  }
  onMount(load);

  // Pick up profile switches made elsewhere (sidebar).
  $effect(() => {
    const p = app.profile;
    if (p && profile && p.name !== profile.name) load();
  });

  const available = $derived(new Set((addons ?? []).map((a) => a.name.toLowerCase())));
  const membership = $derived.by(() => {
    const m = new Map<string, string[]>();
    const walk = (n: GroupNode, g: string) => (n.type === "group" ? n.children.forEach((c) => walk(c, g)) : m.set(n.name.toLowerCase(), [...(m.get(n.name.toLowerCase()) ?? []), g]));
    profile?.groups.forEach((g) => g.type === "group" && walk(g, g.name));
    return m;
  });
  const shown = $derived(
    (addons ?? []).filter((a) => {
      const q = filter.toLowerCase();
      if (q && !a.key.includes(q) && !a.display_name.toLowerCase().includes(q)) return false;
      if (source === "local") return a.source.kind === "local";
      if (source === "workshop") return a.source.kind === "workshop";
      if (source === "unused") return !membership.has(a.name.toLowerCase());
      return true;
    }),
  );
  const customGroups = $derived(profile?.groups.filter((g) => g.type === "group" && !g.modset).map((g) => g.name) ?? []);
  const repos = $derived(app.state?.repos ?? []);

  function changed() {
    saveState = "saving";
    clearTimeout(timer);
    timer = setTimeout(save, 450);
  }
  async function save() {
    if (!profile) return;
    try {
      await saveProfile($state.snapshot(profile) as Profile);
      saveState = "saved";
    } catch (e) {
      saveState = "error";
      toastError(e, "Could not save the profile");
    }
  }

  async function newGroup(withPicked = false) {
    if (!profile) return;
    const name = await promptDialog({
      title: "New group",
      label: "Group name",
      value: `Group ${customGroups.length + 1}`,
      confirmLabel: "Create",
      validate: (v) => (profile!.groups.some((g) => g.name.toLowerCase() === v.toLowerCase()) ? "A group with this name exists" : null),
    });
    if (!name) return;
    profile.groups.push({ type: "group", name, children: [], modset: null });
    if (withPicked) addPicked(name);
    else changed();
  }

  function addPicked(target: string) {
    if (!profile) return;
    const g = profile.groups.find((x) => x.type === "group" && x.name === target);
    if (!g || g.type !== "group") return;
    let n = 0;
    for (const name of picked) {
      if (!g.children.some((c) => c.name.toLowerCase() === name.toLowerCase())) {
        g.children.push({ type: "addon", name, selected: true, optional: false });
        n++;
      }
    }
    picked = new Set();
    changed();
    toast(`${n} addon${n === 1 ? "" : "s"} added to ${target}`);
  }

  async function renameGroup(n: GroupNode) {
    if (!profile || n.type !== "group") return;
    const name = await promptDialog({
      title: "Rename group",
      label: "Group name",
      value: n.name,
      confirmLabel: "Rename",
      validate: (v) => (v !== n.name && profile!.groups.some((g) => g.name.toLowerCase() === v.toLowerCase()) ? "A group with this name exists" : null),
    });
    if (!name) return;
    n.name = name;
    changed();
  }

  async function deleteGroup(n: GroupNode) {
    if (!profile) return;
    if (n.type === "group") {
      const ok = await confirmDialog({
        title: n.modset ? `Remove modset ${n.name}?` : `Delete group ${n.name}?`,
        message: n.modset ? "Its addons stay downloaded. You can add the modset again from the repository." : `The group and its ${n.children.length} addon entries are removed from this profile. Files are not deleted.`,
        confirmLabel: n.modset ? "Remove" : "Delete",
        danger: true,
      });
      if (!ok) return;
    }
    profile.groups.splice(profile.groups.indexOf(n), 1);
    changed();
  }

  async function addModset(repo: string, event: string | null) {
    try {
      profile = structuredClone(await call<Profile>("apply_modset", { profile: profile!.name, repo, event }));
      await loadProfile();
      toast(`${event ?? repo} added`, "ok", { detail: "Its addons are selected in this profile." });
    } catch (e) {
      toastError(e);
    }
  }

  async function loadEvents(repo: string) {
    if (events[repo]) return;
    try {
      events[repo] = (await call<RepoInfo>("repo_info", { name: repo })).events;
    } catch {
      events[repo] = "error";
    }
  }
  $effect(() => {
    for (const r of repos) loadEvents(r.name);
  });
  const modsetItems = $derived(
    repos.flatMap((r) => {
      const ev = events[r.name];
      return [
        { label: r.name, icon: Database, hint: "whole repository", onclick: () => addModset(r.name, null) },
        ...(Array.isArray(ev) ? ev.map((e) => ({ label: `${r.name} › ${e.name}`, icon: CalendarDays, hint: "event", onclick: () => addModset(r.name, e.name) })) : []),
      ];
    }),
  );

  // ----------------------------------------------------------- load order --
  let dragFrom = $state<number | null>(null);
  let dragOver = $state<number | null>(null);

  function addFirst(name: string) {
    if (!profile || profile.addon_priority.some((p) => p.toLowerCase() === name.toLowerCase())) return;
    profile.addon_priority.push(name);
    changed();
  }
  function move(i: number, j: number) {
    if (!profile || j < 0 || j >= profile.addon_priority.length || i === j) return;
    const [x] = profile.addon_priority.splice(i, 1);
    profile.addon_priority.splice(j, 0, x);
    changed();
  }
  function togglePick(name: string, on: boolean) {
    const s = new Set(picked);
    if (on) s.add(name);
    else s.delete(name);
    picked = s;
  }
</script>

{#if !profile || !addons}
  <div class="page"><Skeleton block height={60} /><Skeleton lines={8} /></div>
{:else}
  <div class="page wide">
    <PageHeader title="Modsets" subtitle={`Choose which addons load with the game in profile “${profile.name}”.`}>
      {#snippet meta()}
        {#if saveState === "saving"}<span class="row tight"><LoaderCircle size={13} class="spin" />Saving…</span>
        {:else if saveState === "error"}<span class="err-text">Not saved</span>
        {:else}<span class="row tight ok-text"><Check size={13} />Saved</span>{/if}
      {/snippet}
      {#snippet actions()}
        <Menu icon={Database} text="Add repository modset" size="md" items={modsetItems.length ? modsetItems : [{ label: "No repositories yet", onclick: () => (location.hash = "#/repos"), disabled: false }]} />
        <Button icon={FolderPlus} onclick={() => newGroup()}>New group</Button>
      {/snippet}
    </PageHeader>

    <div class="panes">
      <!-- Groups -->
      <section class="card flush pane">
        <header class="ph"><Layers size={16} /><h2>Groups</h2><span class="subtle xs grow">Checked addons load with the game</span></header>
        <div class="pb tree">
          {#if profile.groups.length}
            <GroupTree nodes={profile.groups} {available} onchange={changed} ondelete={deleteGroup} onrename={renameGroup} />
          {:else}
            <EmptyState icon={Layers} title="No modsets" description="Add a repository's modset, or create a group and put addons from the library in it." compact>
              <Button size="sm" icon={FolderPlus} onclick={() => newGroup()}>New group</Button>
            </EmptyState>
          {/if}
        </div>
      </section>

      <!-- Library -->
      <section class="card flush pane">
        <header class="ph"><Package size={16} /><h2>Addon library</h2><span class="subtle xs grow">{addons.length} found</span></header>
        <div class="tools">
          <div class="search"><Search size={14} /><input type="search" placeholder="Search addons…" bind:value={filter} /></div>
          <SegmentedControl size="sm" bind:value={source} options={[{ value: "all", label: "All" }, { value: "local", label: "Folders" }, { value: "workshop", label: "Workshop" }, { value: "unused", label: "Not in a group" }]} />
        </div>
        <div class="pb lib">
          {#each shown as a (a.key)}
            {@const groups = membership.get(a.name.toLowerCase()) ?? []}
            <label class="lrow" class:on={picked.has(a.name)}>
              <input type="checkbox" checked={picked.has(a.name)} onchange={(e) => togglePick(a.name, (e.target as HTMLInputElement).checked)} />
              <span class="grow stack" style="gap:0;min-width:0">
                <span class="mono small ellipsis">{a.name}</span>
                <span class="xs subtle ellipsis" title={a.path}>{a.display_name !== a.name.replace(/^@/, "") ? a.display_name + " · " : ""}{groups.length ? `in ${groups.join(", ")}` : "not in a group"}</span>
              </span>
              {#if a.source.kind === "workshop"}<Badge plain tone="info">Workshop</Badge>{/if}
              <button class="btn ghost icon sm" data-tip="Load first" data-tip-pos="left" aria-label={`Load ${a.name} first`} onclick={(e) => (e.preventDefault(), addFirst(a.name))}><ArrowUpToLine size={14} /></button>
            </label>
          {:else}
            <EmptyState icon={FolderOpen} title={addons.length ? "No matches" : "No addons found"} description={addons.length ? undefined : "Synchronize a repository, or add addon folders in Settings."} compact />
          {/each}
        </div>
        {#if picked.size}
          <div class="pickbar">
            <span class="strong small grow">{picked.size} selected</span>
            <Button size="sm" variant="ghost" onclick={() => (picked = new Set())}>Clear</Button>
            <Menu
              text={`Add to group`}
              icon={Plus}
              items={[...customGroups.map((g) => ({ label: g, icon: Layers, onclick: () => addPicked(g) })), ...(customGroups.length ? [{ divider: true as const }] : []), { label: "New group…", icon: FolderPlus, onclick: () => newGroup(true) }]}
            />
          </div>
        {/if}
      </section>

      <!-- Load order -->
      <section class="card flush pane">
        <header class="ph"><ListOrdered size={16} /><h2>Load order</h2></header>
        <p class="xs muted hint">These load first, top to bottom; everything else follows. Drag to reorder. Most modsets don't need this.</p>
        <ol class="pb order">
          {#each profile.addon_priority as name, i (name)}
            <li
              draggable="true"
              class:over={dragOver === i}
              class:dragging={dragFrom === i}
              ondragstart={(e) => ((dragFrom = i), e.dataTransfer?.setData("text/plain", name))}
              ondragover={(e) => (e.preventDefault(), (dragOver = i))}
              ondragleave={() => dragOver === i && (dragOver = null)}
              ondrop={(e) => (e.preventDefault(), dragFrom != null && move(dragFrom, i), (dragFrom = dragOver = null))}
              ondragend={() => (dragFrom = dragOver = null)}
            >
              <GripVertical size={14} class="grip" />
              <span class="num tabular">{i + 1}</span>
              <span class="mono small grow ellipsis" class:err-text={!available.has(name.toLowerCase())}>{name}</span>
              <button class="btn ghost icon sm" aria-label="Move up" disabled={i === 0} onclick={() => move(i, i - 1)}><ChevronUp size={14} /></button>
              <button class="btn ghost icon sm" aria-label="Move down" disabled={i === profile.addon_priority.length - 1} onclick={() => move(i, i + 1)}><ChevronDown size={14} /></button>
              <button class="btn ghost icon sm" aria-label={`Remove ${name} from load order`} onclick={() => (profile!.addon_priority.splice(i, 1), changed())}><X size={14} /></button>
            </li>
          {:else}
            <li class="none xs subtle">Use <ArrowUpToLine size={12} /> in the library to pin an addon here.</li>
          {/each}
        </ol>
      </section>
    </div>
  </div>
{/if}

<style>
  .page.wide { max-width: none; height: calc(100vh - 66px); }
  .panes { display: grid; grid-template-columns: minmax(280px, 1.15fr) minmax(300px, 1.2fr) minmax(240px, 0.85fr); gap: 14px; flex: 1; min-height: 0; }
  @media (max-width: 1150px) { .panes { grid-template-columns: 1fr 1fr; } .panes > :last-child { grid-column: 1 / -1; } .page.wide { height: auto; } }
  .pane { display: flex; flex-direction: column; min-height: 0; }
  .ph { display: flex; align-items: center; gap: 8px; padding: 12px 14px; border-bottom: 1px solid var(--line); }
  .ph h2 { margin: 0; font-size: 15.5px; }
  .ph :global(svg) { color: var(--accent-text); }
  .ph .grow { text-align: right; }
  .pb { flex: 1; overflow: auto; padding: 8px 10px; min-height: 160px; }
  .tools { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border-bottom: 1px solid var(--line); }
  .search { position: relative; }
  .search :global(svg) { position: absolute; left: 9px; top: 50%; transform: translateY(-50%); color: var(--fg-3); }
  .search input { padding-left: 30px; }
  .lib { padding: 4px 6px; }
  .lrow { display: flex; align-items: center; gap: 9px; padding: 6px 8px; border-radius: var(--r-sm); cursor: pointer; }
  .lrow:hover { background: var(--surface-2); }
  .lrow.on { background: var(--accent-soft); }
  .pickbar { display: flex; align-items: center; gap: 8px; padding: 9px 12px; border-top: 1px solid var(--accent-line); background: var(--surface-2); }
  .hint { padding: 8px 14px 0; margin: 0; }
  .order { list-style: none; margin: 0; }
  .order li { display: flex; align-items: center; gap: 6px; padding: 4px 6px; border-radius: var(--r-sm); border: 1px solid transparent; }
  .order li:hover { background: var(--surface-2); }
  .order li.over { border-color: var(--accent); }
  .order li.dragging { opacity: 0.4; }
  .order li :global(.grip) { color: var(--fg-3); cursor: grab; }
  .num { width: 18px; color: var(--fg-3); font-size: 12px; text-align: right; }
  .none { display: block; padding: 10px 4px; }
  .none :global(svg) { vertical-align: -2px; }
  :global(.spin) { animation: axs-spin 0.9s linear infinite; }
</style>
