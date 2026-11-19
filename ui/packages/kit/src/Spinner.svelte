<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  let { size = 18, label = "Loading…", inline = false }: { size?: number; label?: string; inline?: boolean } = $props();
</script>

{#if inline}
  <span class="sp" role="status"><LoaderCircle {size} class="spin" /><span class="sr-only">{label}</span></span>
{:else}
  <div class="sp block" role="status"><LoaderCircle {size} class="spin" /><span class="muted small">{label}</span></div>
{/if}

<style>
  .sp { display: inline-flex; align-items: center; gap: 8px; color: var(--fg-2); }
  .sp.block { display: flex; justify-content: center; padding: var(--s7) var(--s4); }
  .sp :global(.spin) { animation: axs-spin 0.8s linear infinite; color: var(--accent); }
</style>
