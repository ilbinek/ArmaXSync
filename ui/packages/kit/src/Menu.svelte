<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import { EllipsisVertical } from "@lucide/svelte";
  import type { MenuItem } from "./types";

  let {
    items,
    label = "More actions",
    icon: Icon = EllipsisVertical,
    text,
    align = "right",
    size = "sm",
    trigger,
  }: {
    items: MenuItem[];
    label?: string;
    icon?: Component<any>;
    text?: string;
    align?: "left" | "right";
    size?: "sm" | "md";
    trigger?: Snippet;
  } = $props();

  let open = $state(false);
  let root: HTMLDivElement;

  function outside(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
  function key(e: KeyboardEvent) {
    if (!open) return;
    const els = Array.from(root.querySelectorAll<HTMLButtonElement>(".item:not(:disabled)"));
    const i = els.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") {
      open = false;
      root.querySelector<HTMLButtonElement>(".trigger")?.focus();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      els[(i + 1) % els.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      els[(i - 1 + els.length) % els.length]?.focus();
    }
  }
  function toggle(e: MouseEvent) {
    e.stopPropagation();
    open = !open;
    if (open) queueMicrotask(() => root.querySelector<HTMLButtonElement>(".item:not(:disabled)")?.focus());
  }
</script>

<svelte:window onclick={outside} onkeydown={key} />
<div class="menu" bind:this={root}>
  {#if trigger}
    <button type="button" class="trigger plain" aria-haspopup="menu" aria-expanded={open} onclick={toggle}>{@render trigger()}</button>
  {:else}
    <button
      type="button"
      class="btn trigger {size === 'sm' ? 'sm' : ''} {text ? '' : 'icon ghost'}"
      aria-haspopup="menu"
      aria-expanded={open}
      aria-label={text ? undefined : label}
      onclick={toggle}><Icon size={size === "sm" ? 15 : 16} />{text ?? ""}</button
    >
  {/if}
  {#if open}
    <div class="pop {align}" role="menu">
      {#each items as it}
        {#if "divider" in it}
          <div class="sep" role="separator"></div>
        {:else}
          <button
            type="button"
            role="menuitem"
            class="item"
            class:danger={it.danger}
            disabled={it.disabled}
            onclick={(e) => {
              e.stopPropagation();
              open = false;
              it.onclick();
            }}
          >
            {#if it.icon}<it.icon size={15} />{:else}<span class="noic"></span>{/if}
            <span class="grow">{it.label}</span>
            {#if it.hint}<span class="subtle xs">{it.hint}</span>{/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .menu { position: relative; display: inline-flex; }
  .plain { background: none; border: 0; padding: 0; color: inherit; font: inherit; cursor: pointer; text-align: left; width: 100%; }
  .pop {
    position: absolute; top: calc(100% + 4px); z-index: 60; min-width: 190px; padding: 4px;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r); box-shadow: var(--shadow-lg);
    animation: in 120ms var(--ease);
  }
  .pop.right { right: 0; }
  .pop.left { left: 0; }
  .item {
    display: flex; align-items: center; gap: 9px; width: 100%; padding: 7px 9px; border: 0; border-radius: var(--r-sm);
    background: none; color: var(--fg); font: inherit; font-size: 13.5px; text-align: left; cursor: pointer; white-space: nowrap;
  }
  .item :global(svg) { color: var(--fg-2); }
  .item:hover:not(:disabled), .item:focus-visible { background: var(--surface-3); outline: none; }
  .item.danger, .item.danger :global(svg) { color: var(--err); }
  .item:disabled { opacity: 0.45; cursor: not-allowed; }
  .noic { width: 15px; }
  .sep { height: 1px; background: var(--line); margin: 4px 2px; }
  @keyframes in { from { opacity: 0; transform: translateY(-4px); } }
</style>
