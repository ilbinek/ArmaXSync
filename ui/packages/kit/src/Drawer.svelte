<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";
  import { trapFocus } from "./focus";

  let {
    open = $bindable(false),
    title = "",
    subtitle = "",
    width = 560,
    persistent = false,
    onclose,
    header,
    children,
    footer,
  }: {
    open?: boolean;
    title?: string;
    subtitle?: string;
    width?: number;
    persistent?: boolean;
    onclose?: () => void;
    header?: Snippet;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  function close() {
    if (persistent) return;
    open = false;
    onclose?.();
  }
</script>

{#if open}
  <div class="backdrop" role="presentation" onclick={close}></div>
  <div
    class="drawer"
    style="width:min({width}px, 100vw)"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    use:trapFocus
    onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), close())}
  >
    <header>
      <div class="t">
        <h2>{title}</h2>
        {#if subtitle}<div class="muted small">{subtitle}</div>{/if}
        {@render header?.()}
      </div>
      <button class="btn ghost icon sm" onclick={close} aria-label="Close" disabled={persistent}><X size={16} /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; background: var(--overlay); z-index: 50; animation: fade var(--dur) var(--ease); }
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 51; display: flex; flex-direction: column;
    background: var(--surface-1); border-left: 1px solid var(--line-strong); box-shadow: var(--shadow-lg);
    animation: slide 180ms var(--ease); outline: none;
  }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; padding: 16px 18px 12px; border-bottom: 1px solid var(--line); }
  .t { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  header h2 { margin: 0; overflow-wrap: anywhere; }
  .body { flex: 1; overflow: auto; padding: 16px 18px; display: flex; flex-direction: column; gap: 14px; }
  footer { padding: 12px 18px; border-top: 1px solid var(--line); display: flex; gap: 8px; justify-content: flex-end; background: var(--surface-2); flex-wrap: wrap; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes slide { from { transform: translateX(40px); opacity: 0; } }
</style>
