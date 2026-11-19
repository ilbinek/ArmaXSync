<script lang="ts">
  // Brand mark: two interlocking sync chevrons inside a reticle.
  let { size = 28, wordmark = false, sub = "" }: { size?: number; wordmark?: boolean; sub?: string } = $props();
</script>

<span class="logo">
  <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
    <rect x="1" y="1" width="30" height="30" rx="6" fill="var(--accent-soft)" stroke="var(--accent-line)" />
    <path d="M16 4v4M16 24v4M4 16h4M24 16h4" stroke="var(--accent)" stroke-width="2" stroke-linecap="round" />
    <path d="M10.5 11.5l5.5 4.5-5.5 4.5" fill="none" stroke="var(--fg)" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" />
    <path d="M21.5 11.5l-5.5 4.5 5.5 4.5" fill="none" stroke="var(--accent)" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
  {#if wordmark}
    <span class="wm">
      <span class="name">Arma<b>X</b>Sync</span>
      {#if sub}<span class="sub">{sub}</span>{/if}
    </span>
  {/if}
</span>

<style>
  .logo { display: inline-flex; align-items: center; gap: 10px; color: var(--fg); }
  .wm { display: flex; flex-direction: column; line-height: 1; }
  .name { font-family: var(--font-display); font-weight: 700; font-size: 20px; letter-spacing: 0.06em; text-transform: uppercase; }
  .name b { color: var(--accent); font-weight: 700; }
  .sub { font-family: var(--font-display); font-size: 11px; font-weight: 600; letter-spacing: 0.18em; text-transform: uppercase; color: var(--fg-3); margin-top: 3px; }
</style>
