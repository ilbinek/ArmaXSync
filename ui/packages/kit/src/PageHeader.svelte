<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import { ChevronRight } from "@lucide/svelte";

  let {
    title,
    subtitle,
    eyebrow,
    crumbs = [],
    icon: Icon,
    actions,
    meta,
    children,
  }: {
    title: string;
    subtitle?: string;
    eyebrow?: string;
    crumbs?: { label: string; href?: string }[];
    icon?: Component<any>;
    actions?: Snippet;
    meta?: Snippet;
    children?: Snippet;
  } = $props();
</script>

<header class="ph">
  {#if crumbs.length}
    <nav class="crumbs" aria-label="Breadcrumb">
      {#each crumbs as c}
        <!-- crumbs are the parents of this page; each one leads into the next -->
        {#if c.href}<a href={c.href}>{c.label}</a>{:else}<span>{c.label}</span>{/if}<ChevronRight size={13} />
      {/each}
    </nav>
  {:else if eyebrow}
    <div class="eyebrow">{eyebrow}</div>
  {/if}
  <div class="main">
    {#if Icon}<div class="ic"><Icon size={22} /></div>{/if}
    <div class="titles">
      <h1>{title}</h1>
      {#if subtitle}<div class="sub muted">{subtitle}</div>{/if}
      {#if meta}<div class="meta">{@render meta()}</div>{/if}
    </div>
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </div>
  {@render children?.()}
</header>

<style>
  .ph { display: flex; flex-direction: column; gap: 6px; }
  .crumbs { display: flex; align-items: center; gap: 4px; font-size: 12.5px; color: var(--fg-3); flex-wrap: wrap; }
  .crumbs a { color: var(--fg-2); }
  .crumbs :global(svg) { opacity: 0.6; }
  .main { display: flex; align-items: flex-start; gap: var(--s3); flex-wrap: wrap; }
  .ic {
    width: 44px; height: 44px; display: grid; place-items: center; flex-shrink: 0;
    border: 1px solid var(--accent-line); background: var(--accent-soft); color: var(--accent-text); border-radius: var(--r-lg);
  }
  .titles { flex: 1; min-width: 220px; display: flex; flex-direction: column; gap: 4px; }
  .titles h1 { overflow-wrap: anywhere; }
  .sub { font-size: 13.5px; }
  .meta { display: flex; flex-wrap: wrap; gap: 6px 14px; align-items: center; font-size: 12.5px; color: var(--fg-2); }
  .actions { display: flex; gap: var(--s2); flex-wrap: wrap; align-items: center; }
</style>
