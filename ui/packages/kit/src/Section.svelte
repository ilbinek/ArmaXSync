<script lang="ts">
  import type { Component, Snippet } from "svelte";

  let {
    title,
    description,
    icon: Icon,
    actions,
    footer,
    flush = false,
    id,
    class: cls = "",
    children,
  }: {
    title?: string;
    description?: string;
    icon?: Component<any>;
    actions?: Snippet;
    footer?: Snippet;
    flush?: boolean;
    id?: string;
    class?: string;
    children: Snippet;
  } = $props();
</script>

<section class="card sec {cls}" class:flush {id}>
  {#if title || actions}
    <div class="head" class:pad={flush}>
      <div class="t">
        {#if title}<h2>{#if Icon}<Icon size={16} />{/if}{title}</h2>{/if}
        {#if description}<p class="muted small">{description}</p>{/if}
      </div>
      {#if actions}<div class="row">{@render actions()}</div>{/if}
    </div>
  {/if}
  <div class="body" class:stack={!flush}>{@render children()}</div>
  {#if footer}<div class="foot">{@render footer()}</div>{/if}
</section>

<style>
  .sec { display: flex; flex-direction: column; gap: var(--s4); }
  .sec.flush { gap: 0; }
  .head { display: flex; gap: var(--s3); align-items: flex-start; justify-content: space-between; flex-wrap: wrap; }
  .head.pad { padding: var(--s4) var(--s4) var(--s3); border-bottom: 1px solid var(--line); }
  .t { display: flex; flex-direction: column; gap: 4px; min-width: 0; flex: 1; }
  .t h2 { margin: 0; display: flex; align-items: center; gap: 8px; }
  .t h2 :global(svg) { color: var(--accent-text); }
  .t p { margin: 0; max-width: 72ch; }
  .foot { display: flex; justify-content: flex-end; gap: var(--s2); padding-top: var(--s3); border-top: 1px solid var(--line); flex-wrap: wrap; }
  .flush .foot { padding: var(--s3) var(--s4); }
</style>
