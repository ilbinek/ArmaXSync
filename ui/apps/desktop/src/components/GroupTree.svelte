<script lang="ts">
  import GroupTree from "./GroupTree.svelte";
  import { Menu } from "@axs/kit";
  import { ChevronRight, Database, CalendarDays, FolderClosed, Pencil, Trash2, X, CircleAlert } from "@lucide/svelte";
  import type { GroupNode } from "../api";

  let {
    nodes,
    available,
    readonly = false,
    depth = 0,
    onchange,
    ondelete,
    onrename,
  }: {
    nodes: GroupNode[];
    available: Set<string>;
    readonly?: boolean;
    depth?: number;
    onchange: () => void;
    ondelete: (n: GroupNode) => void;
    onrename?: (n: GroupNode) => void;
  } = $props();

  let collapsed = $state<Record<string, boolean>>({});

  function count(n: GroupNode): [number, number] {
    if (n.type === "addon") return [n.selected ? 1 : 0, 1];
    return n.children.reduce(([a, b], c) => {
      const [x, y] = count(c);
      return [a + x, b + y];
    }, [0, 0]);
  }
  function allSelected(n: GroupNode): boolean {
    return n.type === "addon" ? n.selected : n.children.length > 0 && n.children.every(allSelected);
  }
  function someSelected(n: GroupNode): boolean {
    return n.type === "addon" ? n.selected : n.children.some(someSelected);
  }
  function setSel(n: GroupNode, v: boolean) {
    if (n.type === "addon") n.selected = v;
    else n.children.forEach((c) => setSel(c, v));
  }
</script>

<ul class:root={depth === 0}>
  {#each nodes as n (n.type + n.name)}
    {#if n.type === "group"}
      {@const [sel, tot] = count(n)}
      {@const isModset = !!n.modset}
      <li class="group">
        <div class="node">
          <button class="tw" aria-label={collapsed[n.name] ? "Expand" : "Collapse"} onclick={() => (collapsed[n.name] = !collapsed[n.name])}>
            <ChevronRight size={14} style={collapsed[n.name] ? "" : "transform:rotate(90deg)"} />
          </button>
          <input type="checkbox" checked={allSelected(n)} indeterminate={!allSelected(n) && someSelected(n)} aria-label={`Select all in ${n.name}`} onchange={(e) => (setSel(n, (e.target as HTMLInputElement).checked), onchange())} />
          <span class="gic">
            {#if n.modset?.kind === "event"}<CalendarDays size={15} />{:else if n.modset}<Database size={15} />{:else}<FolderClosed size={15} />{/if}
          </span>
          <span class="gname ellipsis">{n.name}</span>
          <span class="cnt tabular">{sel}/{tot}</span>
          {#if !readonly}
            <Menu
              items={[
                ...(!isModset && onrename ? [{ label: "Rename", icon: Pencil, onclick: () => onrename(n) }] : []),
                { label: isModset ? "Remove modset" : "Delete group", icon: Trash2, danger: true, onclick: () => ondelete(n) },
              ]}
            />
          {/if}
        </div>
        {#if !collapsed[n.name]}
          {#if n.children.length}
            <GroupTree
              nodes={n.children}
              {available}
              readonly={readonly || isModset}
              depth={depth + 1}
              {onchange}
              ondelete={(c) => {
                n.children.splice(n.children.indexOf(c), 1);
                onchange();
              }}
              {onrename}
            />
          {:else}
            <div class="emptyg xs subtle">Empty. Select addons in the library and add them here.</div>
          {/if}
        {/if}
      </li>
    {:else}
      {@const missing = !available.has(n.name.toLowerCase())}
      <li class="addon" class:missing>
        <div class="node">
          <span class="tw"></span>
          <input type="checkbox" bind:checked={n.selected} aria-label={`Load ${n.name}`} onchange={onchange} />
          <span class="aname mono ellipsis">{n.name}</span>
          {#if n.optional}<span class="badge plain">optional</span>{/if}
          {#if missing}<span class="miss" title="Not found in your addon folders"><CircleAlert size={13} />missing</span>{/if}
          {#if !readonly}
            <button class="btn ghost icon sm del" aria-label={`Remove ${n.name}`} onclick={() => ondelete(n)}><X size={13} /></button>
          {/if}
        </div>
      </li>
    {/if}
  {/each}
</ul>

<style>
  ul { list-style: none; margin: 0; padding-left: 22px; }
  ul.root { padding-left: 0; }
  .group + .group { margin-top: 2px; }
  .node { display: flex; align-items: center; gap: 7px; min-height: 30px; padding: 0 4px; border-radius: var(--r-sm); }
  .node:hover { background: var(--surface-2); }
  .tw { width: 18px; height: 18px; display: grid; place-items: center; background: none; border: 0; padding: 0; color: var(--fg-3); cursor: pointer; flex-shrink: 0; }
  .gic { color: var(--accent-text); display: grid; place-items: center; }
  .gname { font-weight: 600; flex: 1; min-width: 0; }
  .cnt { font-size: 11.5px; color: var(--fg-3); }
  .aname { font-size: 12.5px; flex: 1; min-width: 0; }
  .missing .aname { color: var(--err); }
  .miss { display: inline-flex; align-items: center; gap: 3px; font-size: 11.5px; color: var(--err); }
  .del { opacity: 0; }
  .node:hover .del, .del:focus-visible { opacity: 1; }
  .emptyg { padding: 4px 0 6px 50px; }
</style>
