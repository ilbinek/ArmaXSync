<script lang="ts">
  import type { Snippet } from "svelte";
  let { open = $bindable(false), title = "", wide = false, children, footer }: { open?: boolean; title?: string; wide?: boolean; children: Snippet; footer?: Snippet } = $props();
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") open = false;
  }
</script>

<svelte:window onkeydown={open ? key : undefined} />
{#if open}
  <div class="backdrop" role="presentation" onclick={() => (open = false)}></div>
  <div class="modal" class:wide role="dialog" aria-modal="true" aria-label={title}>
    <header><h2>{title}</h2><button class="btn ghost small" onclick={() => (open = false)} aria-label="Close">✕</button></header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; background: rgb(0 0 0 / 0.45); z-index: 50; }
  .modal { position: fixed; z-index: 51; top: 50%; left: 50%; transform: translate(-50%, -50%); width: min(520px, calc(100vw - 32px));
    max-height: calc(100vh - 48px); display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; box-shadow: 0 10px 40px rgb(0 0 0 / 0.3); }
  .modal.wide { width: min(860px, calc(100vw - 32px)); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; border-bottom: 1px solid var(--line); }
  header h2 { margin: 0; }
  .body { padding: 16px; overflow: auto; display: flex; flex-direction: column; gap: 12px; }
  footer { padding: 12px 16px; border-top: 1px solid var(--line); display: flex; justify-content: flex-end; gap: 8px; }
</style>
