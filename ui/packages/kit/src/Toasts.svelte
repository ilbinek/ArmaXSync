<script lang="ts">
  import { toasts } from "./toast.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}">{t.text}</div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 16px; bottom: 16px; display: flex; flex-direction: column; gap: 8px; z-index: 100; max-width: min(420px, calc(100vw - 32px)); }
  .toast { background: var(--panel); border: 1px solid var(--line); border-left: 4px solid var(--accent); border-radius: 7px; padding: 10px 14px; box-shadow: 0 4px 18px rgb(0 0 0 / 0.15); }
  .toast.ok { border-left-color: var(--ok); }
  .toast.err { border-left-color: var(--err); }
</style>
