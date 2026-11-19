<script lang="ts">
  import { X } from "@lucide/svelte";
  let {
    values = $bindable([]),
    placeholder = "Type and press Enter",
    id,
    mono = true,
    onchange,
  }: { values?: string[]; placeholder?: string; id?: string; mono?: boolean; onchange?: (v: string[]) => void } = $props();

  let draft = $state("");

  function add(raw: string) {
    const parts = raw
      .split(/[\n,]/)
      .map((s) => s.trim())
      .filter(Boolean)
      .filter((s) => !values.includes(s));
    if (!parts.length) return;
    values = [...values, ...parts];
    onchange?.(values);
  }
  function remove(i: number) {
    values = values.filter((_, j) => j !== i);
    onchange?.(values);
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      add(draft);
      draft = "";
    } else if (e.key === "Backspace" && !draft && values.length) {
      remove(values.length - 1);
    }
  }
  function paste(e: ClipboardEvent) {
    const t = e.clipboardData?.getData("text") ?? "";
    if (/[\n,]/.test(t)) {
      e.preventDefault();
      add(t);
    }
  }
</script>

<div class="tl" class:mono>
  {#each values as v, i (v)}
    <span class="chip">{v}<button type="button" aria-label={`Remove ${v}`} onclick={() => remove(i)}><X size={12} /></button></span>
  {/each}
  <input
    {id}
    bind:value={draft}
    placeholder={values.length ? "" : placeholder}
    onkeydown={key}
    onpaste={paste}
    onblur={() => {
      add(draft);
      draft = "";
    }}
  />
</div>

<style>
  .tl {
    display: flex; flex-wrap: wrap; gap: 5px; align-items: center; min-height: var(--control-h); padding: 3px 5px;
    border: 1px solid var(--line-strong); border-radius: var(--r); background: var(--bg);
  }
  .tl:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); }
  .tl.mono .chip { font-family: var(--mono); font-size: 12px; }
  .chip { height: 22px; padding: 0 4px 0 8px; border-radius: var(--r-sm); }
  .chip button { display: grid; place-items: center; width: 16px; height: 16px; border: 0; border-radius: 2px; background: none; color: var(--fg-3); cursor: pointer; padding: 0; }
  .chip button:hover { color: var(--err); background: var(--err-soft); }
  input { flex: 1; min-width: 140px; border: 0 !important; box-shadow: none !important; background: transparent; height: 24px; padding: 0 4px; }
</style>
