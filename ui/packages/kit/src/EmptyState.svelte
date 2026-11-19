<script lang="ts">
  import type { Component, Snippet } from "svelte";
  let {
    icon: Icon,
    title,
    description,
    compact = false,
    children,
  }: { icon?: Component<any>; title: string; description?: string; compact?: boolean; children?: Snippet } = $props();
</script>

<div class="es" class:compact>
  {#if Icon}<div class="ic"><Icon size={compact ? 20 : 26} /></div>{/if}
  <div class="t">{title}</div>
  {#if description}<p class="muted">{description}</p>{/if}
  {#if children}<div class="row" style="justify-content:center">{@render children()}</div>{/if}
</div>

<style>
  .es { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 10px; padding: var(--s8) var(--s4); }
  .es.compact { padding: var(--s6) var(--s3); gap: 8px; }
  .ic {
    width: 56px; height: 56px; display: grid; place-items: center; border-radius: 50%;
    background: var(--surface-2); border: 1px dashed var(--line-strong); color: var(--fg-3);
  }
  .compact .ic { width: 42px; height: 42px; }
  .t { font-family: var(--font-display); font-size: 18px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; }
  .compact .t { font-size: 15px; }
  p { margin: 0; max-width: 52ch; font-size: 13.5px; }
</style>
