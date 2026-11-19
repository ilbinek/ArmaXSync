<script lang="ts">
  import { FolderOpen } from "@lucide/svelte";
  let {
    value = $bindable(""),
    id,
    placeholder = "",
    browse,
    browseLabel = "Browse…",
    disabled = false,
    invalid = false,
    onchange,
  }: {
    value?: string | null;
    id?: string;
    placeholder?: string;
    /** Opens a picker; returns the chosen path or null. */
    browse?: () => Promise<string | null | undefined>;
    browseLabel?: string;
    disabled?: boolean;
    invalid?: boolean;
    onchange?: (v: string) => void;
  } = $props();

  async function pick() {
    const p = await browse?.();
    if (p) {
      value = p;
      onchange?.(p);
    }
  }
</script>

<div class="input-group">
  <input {id} class="mono-in" bind:value {placeholder} {disabled} aria-invalid={invalid} spellcheck="false" onchange={() => onchange?.(value ?? "")} />
  {#if browse}<button type="button" class="btn" onclick={pick} {disabled}><FolderOpen size={15} />{browseLabel}</button>{/if}
</div>

<style>
  .mono-in { font-family: var(--mono); font-size: 12.5px; }
</style>
