<script lang="ts" generics="T extends string">
  import type { Component } from "svelte";
  let {
    options,
    value = $bindable(),
    size = "md",
    label,
    onchange,
  }: {
    options: { value: T; label: string; icon?: Component<any>; disabled?: boolean }[];
    value?: T;
    size?: "sm" | "md";
    label?: string;
    onchange?: (v: T) => void;
  } = $props();
</script>

<div class="seg {size}" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    <button
      type="button"
      role="radio"
      aria-checked={value === o.value}
      class:on={value === o.value}
      disabled={o.disabled}
      onclick={() => {
        value = o.value;
        onchange?.(o.value);
      }}>{#if o.icon}<o.icon size={14} />{/if}{o.label}</button
    >
  {/each}
</div>

<style>
  .seg { display: inline-flex; padding: 2px; gap: 2px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r); flex-wrap: wrap; }
  button {
    display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 11px; border: 0; border-radius: var(--r-sm);
    background: transparent; color: var(--fg-2); font: inherit; font-size: 13px; font-weight: 550; cursor: pointer; white-space: nowrap;
  }
  .sm button { height: 22px; padding: 0 8px; font-size: 12px; }
  button:hover:not(:disabled):not(.on) { color: var(--fg); background: var(--surface-3); }
  button.on { background: var(--surface-3); color: var(--fg); box-shadow: 0 0 0 1px var(--line-strong) inset; }
  button.on :global(svg) { color: var(--accent-text); }
  button:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
