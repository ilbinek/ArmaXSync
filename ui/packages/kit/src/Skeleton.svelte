<script lang="ts">
  let { lines = 3, height = 14, width = "100%", block = false }: { lines?: number; height?: number; width?: string; block?: boolean } = $props();
</script>

{#if block}
  <div class="sk" style="height:{height}px;width:{width}" aria-hidden="true"></div>
{:else}
  <div class="stack tight" aria-busy="true" aria-label="Loading">
    {#each Array(lines) as _, i}
      <div class="sk" style="height:{height}px;width:{i === lines - 1 && lines > 1 ? '60%' : width}"></div>
    {/each}
  </div>
{/if}

<style>
  .sk {
    border-radius: var(--r-sm);
    background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
    background-size: 200% 100%;
    animation: shimmer 1.4s ease-in-out infinite;
  }
  @keyframes shimmer { from { background-position: 100% 0; } to { background-position: -100% 0; } }
</style>
