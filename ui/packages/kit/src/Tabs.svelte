<script lang="ts">
  import type { TabItem } from "./types";
  import { router, setQuery } from "./router.svelte";

  let {
    items,
    value = $bindable(),
    param,
    onchange,
  }: {
    items: TabItem[];
    value?: string;
    /** Keep the active tab in this URL query parameter (e.g. "tab"). */
    param?: string;
    onchange?: (id: string) => void;
  } = $props();

  const visible = $derived(items.filter((i) => !i.hidden));

  // Initial value from the URL, else the first tab.
  $effect.pre(() => {
    if (value == null || !visible.some((i) => i.id === value)) {
      const q = param ? router.query.get(param) : null;
      value = visible.find((i) => i.id === q)?.id ?? visible[0]?.id;
    }
  });

  function select(id: string) {
    if (id === value) return;
    value = id;
    if (param) setQuery(param, id === visible[0]?.id ? null : id);
    onchange?.(id);
  }

  function key(e: KeyboardEvent) {
    const i = visible.findIndex((t) => t.id === value);
    let n = -1;
    if (e.key === "ArrowRight") n = (i + 1) % visible.length;
    else if (e.key === "ArrowLeft") n = (i - 1 + visible.length) % visible.length;
    else if (e.key === "Home") n = 0;
    else if (e.key === "End") n = visible.length - 1;
    if (n < 0) return;
    e.preventDefault();
    select(visible[n].id);
    (e.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>("[role=tab]")[n]?.focus();
  }
</script>

<div class="tabs" role="tablist" tabindex="-1" onkeydown={key}>
  {#each visible as t (t.id)}
    <button role="tab" aria-selected={t.id === value} tabindex={t.id === value ? 0 : -1} class:active={t.id === value} onclick={() => select(t.id)}>
      {#if t.icon}<t.icon size={15} />{/if}
      {t.label}
      {#if t.count != null && t.count !== 0 && t.count !== ""}<span class="count">{t.count}</span>{/if}
    </button>
  {/each}
</div>
