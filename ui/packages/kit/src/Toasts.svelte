<script lang="ts">
  import { CircleCheck, CircleAlert, Info, TriangleAlert, X } from "@lucide/svelte";
  import { toasts, dismissToast } from "./toast.svelte";
  import DialogHost from "./DialogHost.svelte";

  const icons = { ok: CircleCheck, err: CircleAlert, info: Info, warn: TriangleAlert };
</script>

<DialogHost />
<div class="toasts" aria-live="polite" aria-relevant="additions">
  {#each toasts as t (t.id)}
    {@const Icon = icons[t.kind]}
    <div class="toast {t.kind}" role={t.kind === "err" ? "alert" : "status"}>
      <Icon size={17} />
      <div class="txt">
        <div>{t.text}{#if t.count > 1}<span class="n">×{t.count}</span>{/if}</div>
        {#if t.detail}<div class="detail">{t.detail}</div>{/if}
      </div>
      {#if t.action}
        <button class="btn sm" onclick={() => { t.action!.run(); dismissToast(t.id); }}>{t.action.label}</button>
      {/if}
      <button class="btn ghost icon sm x" aria-label="Dismiss" onclick={() => dismissToast(t.id)}><X size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 16px; bottom: 16px; display: flex; flex-direction: column; gap: 8px; z-index: 100; width: min(420px, calc(100vw - 32px)); }
  .toast {
    display: flex; align-items: flex-start; gap: 10px; padding: 10px 8px 10px 12px;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-left: 3px solid var(--accent);
    border-radius: var(--r); box-shadow: var(--shadow-lg); animation: in 180ms var(--ease);
  }
  .toast > :global(svg) { margin-top: 1px; color: var(--accent); }
  .toast.ok { border-left-color: var(--ok); }
  .toast.ok > :global(svg) { color: var(--ok); }
  .toast.err { border-left-color: var(--err); }
  .toast.err > :global(svg) { color: var(--err); }
  .toast.warn { border-left-color: var(--warn); }
  .toast.warn > :global(svg) { color: var(--warn); }
  .toast.info { border-left-color: var(--info); }
  .toast.info > :global(svg) { color: var(--info); }
  .txt { flex: 1; min-width: 0; overflow-wrap: anywhere; font-size: 13.5px; }
  .detail { color: var(--fg-2); font-size: 12.5px; margin-top: 2px; }
  .n { margin-left: 6px; font-size: 11px; font-weight: 700; color: var(--fg-3); }
  .x { margin-top: -3px; }
  @keyframes in { from { opacity: 0; transform: translateY(8px); } }
</style>
