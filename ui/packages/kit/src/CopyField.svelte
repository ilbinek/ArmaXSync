<script lang="ts">
  import { Copy, Check, ExternalLink } from "@lucide/svelte";
  import { copyText } from "./clipboard";
  import { toast } from "./toast.svelte";

  let {
    value,
    label,
    hint,
    secret = false,
    href,
  }: { value: string; label?: string; hint?: string; secret?: boolean; href?: string } = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout>;

  async function copy() {
    if (await copyText(value)) {
      copied = true;
      clearTimeout(timer);
      timer = setTimeout(() => (copied = false), 1600);
    } else toast("Could not copy: select the text and press Ctrl+C", "warn");
  }
</script>

<div class="cf">
  {#if label}<div class="lbl">{label}</div>{/if}
  <div class="box" class:secret>
    <code title={value}>{value}</code>
    {#if href}<a class="btn ghost icon sm" {href} target="_blank" rel="noopener" aria-label="Open"><ExternalLink size={14} /></a>{/if}
    <button type="button" class="btn sm" class:copied onclick={copy} aria-label={`Copy ${label ?? ""}`}>
      {#if copied}<Check size={14} />Copied{:else}<Copy size={14} />Copy{/if}
    </button>
  </div>
  {#if hint}<div class="hint">{hint}</div>{/if}
</div>

<style>
  .cf { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .lbl { font-weight: 550; font-size: 13px; }
  .box { display: flex; align-items: center; gap: 6px; padding: 4px 4px 4px 10px; border: 1px solid var(--line); border-radius: var(--r); background: var(--bg); min-width: 0; }
  code { flex: 1; min-width: 0; background: none; border: 0; padding: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; word-break: normal; color: var(--fg); }
  .secret code { color: var(--accent-text); }
  .copied { color: var(--ok); border-color: var(--ok); }
  .hint { color: var(--fg-2); font-size: 12.5px; }
</style>
