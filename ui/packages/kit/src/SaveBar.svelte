<script lang="ts">
  import { CircleDot } from "@lucide/svelte";
  import Button from "./Button.svelte";

  let {
    dirty,
    saving = false,
    error = null,
    message = "Unsaved changes",
    saveLabel = "Save changes",
    onsave,
    ondiscard,
  }: {
    dirty: boolean;
    saving?: boolean;
    error?: string | null;
    message?: string;
    saveLabel?: string;
    onsave: () => void;
    ondiscard?: () => void;
  } = $props();
</script>

<svelte:window
  onkeydown={(e) => {
    if (dirty && !saving && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      onsave();
    }
  }}
/>

{#if dirty || saving}
  <div class="sb" role="region" aria-label="Unsaved changes">
    <CircleDot size={15} />
    <span class="grow">{#if error}<span class="err-text">{error}</span>{:else}{message}{/if}</span>
    <span class="subtle xs hide-sm"><kbd>Ctrl</kbd> <kbd>S</kbd></span>
    {#if ondiscard}<Button variant="ghost" size="sm" onclick={ondiscard} disabled={saving}>Discard</Button>{/if}
    <Button variant="primary" size="sm" loading={saving} onclick={onsave}>{saveLabel}</Button>
  </div>
{/if}

<style>
  .sb {
    position: sticky; bottom: 12px; z-index: 20; display: flex; align-items: center; gap: 10px;
    margin-top: var(--s2); padding: 8px 10px 8px 14px; border-radius: var(--r-lg);
    background: var(--surface-2); border: 1px solid var(--accent-line); box-shadow: var(--shadow-lg);
    animation: up 160ms var(--ease);
  }
  .sb > :global(svg) { color: var(--accent); }
  @keyframes up { from { opacity: 0; transform: translateY(8px); } }
</style>
