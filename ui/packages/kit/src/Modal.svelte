<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";
  import { trapFocus } from "./focus";

  let {
    open = $bindable(false),
    title = "",
    description = "",
    wide = false,
    size,
    /** Ignore Escape / backdrop clicks (e.g. while busy or with unsaved input). */
    persistent = false,
    onclose,
    children,
    footer,
  }: {
    open?: boolean;
    title?: string;
    description?: string;
    wide?: boolean;
    size?: "sm" | "md" | "lg";
    persistent?: boolean;
    onclose?: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  const sz = $derived(size ?? (wide ? "lg" : "md"));

  function close() {
    if (persistent) return;
    open = false;
    onclose?.();
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  }
</script>

{#if open}
  <div class="backdrop" role="presentation" onclick={close}></div>
  <div class="modal {sz}" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" use:trapFocus onkeydown={key}>
    <header>
      <div class="t">
        <h2>{title}</h2>
        {#if description}<p class="muted small">{description}</p>{/if}
      </div>
      <button class="btn ghost icon sm" onclick={close} aria-label="Close" disabled={persistent}><X size={16} /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; background: var(--overlay); z-index: 50; animation: fade var(--dur) var(--ease); backdrop-filter: blur(2px); }
  .modal {
    position: fixed; z-index: 51; top: 50%; left: 50%; transform: translate(-50%, -50%);
    width: min(540px, calc(100vw - 24px)); max-height: calc(100vh - 40px); display: flex; flex-direction: column;
    background: var(--surface-1); border: 1px solid var(--line-strong); border-top: 2px solid var(--accent); border-radius: var(--r-lg);
    box-shadow: var(--shadow-lg); animation: pop var(--dur) var(--ease); outline: none;
  }
  .modal.sm { width: min(420px, calc(100vw - 24px)); }
  .modal.lg { width: min(880px, calc(100vw - 24px)); }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; padding: 14px 16px 12px; border-bottom: 1px solid var(--line); }
  .t { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  header h2 { margin: 0; }
  header p { margin: 0; }
  .body { padding: 16px; overflow: auto; display: flex; flex-direction: column; gap: 14px; }
  footer { padding: 12px 16px; border-top: 1px solid var(--line); display: flex; justify-content: flex-end; gap: 8px; flex-wrap: wrap; background: var(--surface-2); border-radius: 0 0 var(--r-lg) var(--r-lg); }
  @keyframes fade { from { opacity: 0; } }
  @keyframes pop { from { opacity: 0; transform: translate(-50%, -48%) scale(0.98); } }
</style>
