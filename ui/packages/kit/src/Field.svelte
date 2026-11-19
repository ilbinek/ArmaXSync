<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    hint,
    error,
    required = false,
    id = `f-${Math.random().toString(36).slice(2, 9)}`,
    class: cls = "",
    children,
    aside,
  }: {
    label?: string;
    hint?: string;
    error?: string | null;
    required?: boolean;
    id?: string;
    class?: string;
    /** Receives the input id to put on the control. */
    children: Snippet<[string]>;
    /** Right-aligned extra next to the label (e.g. a link). */
    aside?: Snippet;
  } = $props();
</script>

<div class="field {cls}">
  {#if label || aside}
    <div class="lrow">
      {#if label}<label for={id}>{label}{#if required}<span class="req" aria-hidden="true">*</span>{/if}</label>{/if}
      {#if aside}<span class="aside">{@render aside()}</span>{/if}
    </div>
  {/if}
  {@render children(id)}
  {#if error}<span class="error" role="alert">{error}</span>{:else if hint}<span class="hint">{hint}</span>{/if}
</div>

<style>
  .lrow { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; }
  .lrow label { font-weight: 550; font-size: 13px; }
  .aside { font-size: 12.5px; }
</style>
