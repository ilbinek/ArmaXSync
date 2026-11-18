<script lang="ts">
  import GroupTree from "./GroupTree.svelte";
  import type { GroupNode } from "../api";

  let {
    nodes,
    available,
    readonly = false,
    onchange,
    ondelete,
  }: { nodes: GroupNode[]; available: Set<string>; readonly?: boolean; onchange: () => void; ondelete: (n: GroupNode) => void } = $props();

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

<ul>
  {#each nodes as n, i}
    <li>
      <div class="node">
        <input
          type="checkbox"
          checked={allSelected(n)}
          indeterminate={!allSelected(n) && someSelected(n)}
          onchange={(e) => { setSel(n, (e.target as HTMLInputElement).checked); onchange(); }}
        />
        {#if n.type === "group"}
          <b>{n.name}</b>
          {#if n.modset}<span class="badge info">{n.modset.kind === "event" ? "event" : "repository"}</span>{/if}
        {:else}
          <span class:missing={!available.has(n.name.toLowerCase())}>{n.name}</span>
          {#if n.optional}<span class="badge">optional</span>{/if}
          {#if !available.has(n.name.toLowerCase())}<span class="small missing">missing</span>{/if}
        {/if}
        {#if !readonly}<button class="btn ghost small del" title="Remove" onclick={() => ondelete(n)}>✕</button>{/if}
      </div>
      {#if n.type === "group" && n.children.length}
        <GroupTree nodes={n.children} {available} readonly={readonly || !!n.modset} {onchange} ondelete={(c) => { n.children.splice(n.children.indexOf(c), 1); onchange(); }} />
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul { list-style: none; margin: 0; padding-left: 18px; }
  :global(.tree-root) > ul { padding-left: 0; }
  .node { display: flex; align-items: center; gap: 6px; padding: 2px 0; }
  .node .del { visibility: hidden; margin-left: auto; }
  .node:hover .del { visibility: visible; }
  .missing { color: var(--err); }
</style>
