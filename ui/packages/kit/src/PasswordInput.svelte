<script lang="ts">
  import { Eye, EyeOff } from "@lucide/svelte";
  let {
    value = $bindable(""),
    id,
    placeholder,
    autocomplete = "off",
    disabled = false,
    invalid = false,
  }: { value?: string | null; id?: string; placeholder?: string; autocomplete?: any; disabled?: boolean; invalid?: boolean } = $props();
  let show = $state(false);
</script>

<div class="pw">
  {#if show}
    <input type="text" {id} bind:value {placeholder} {disabled} aria-invalid={invalid} autocomplete="off" spellcheck="false" />
  {:else}
    <input type="password" {id} bind:value {placeholder} {disabled} aria-invalid={invalid} {autocomplete} />
  {/if}
  <button type="button" class="btn ghost icon sm eye" onclick={() => (show = !show)} aria-label={show ? "Hide password" : "Show password"} {disabled}>
    {#if show}<EyeOff size={15} />{:else}<Eye size={15} />{/if}
  </button>
</div>

<style>
  .pw { position: relative; }
  .pw input { padding-right: 36px; }
  .eye { position: absolute; right: 3px; top: 50%; transform: translateY(-50%); }
</style>
