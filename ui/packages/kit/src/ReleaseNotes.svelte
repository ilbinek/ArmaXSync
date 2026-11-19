<script lang="ts">
  // Release notes from CHANGELOG.md sections: headings (#…), bullet lists,
  // paragraphs and `code`. Text only, never HTML.
  let { text, max = 0 }: { text: string; max?: number } = $props();

  type Block = { kind: "h" | "p"; text: string } | { kind: "ul"; items: string[] };

  const blocks = $derived.by(() => {
    const out: Block[] = [];
    for (const raw of (text ?? "").split(/\r?\n/)) {
      const line = raw.trimEnd();
      if (!line.trim()) continue;
      const h = line.match(/^#{1,6}\s+(.*)$/);
      const li = line.match(/^\s*[-*]\s+(.*)$/);
      if (h) out.push({ kind: "h", text: h[1] });
      else if (li) {
        const last = out[out.length - 1];
        if (last?.kind === "ul") last.items.push(li[1]);
        else out.push({ kind: "ul", items: [li[1]] });
      } else out.push({ kind: "p", text: line.trim() });
    }
    return max > 0 ? out.slice(0, max) : out;
  });

  // `code` spans → parts
  function parts(s: string) {
    return s.split(/(`[^`]+`)/).map((p) => (p.startsWith("`") && p.endsWith("`") ? { code: true, t: p.slice(1, -1) } : { code: false, t: p.replace(/\*\*/g, "") }));
  }
</script>

<div class="rn">
  {#each blocks as b}
    {#if b.kind === "h"}
      <div class="h">{b.text}</div>
    {:else if b.kind === "ul"}
      <ul>{#each b.items as it}<li>{#each parts(it) as p}{#if p.code}<code>{p.t}</code>{:else}{p.t}{/if}{/each}</li>{/each}</ul>
    {:else}
      <p>{#each parts(b.text) as p}{#if p.code}<code>{p.t}</code>{:else}{p.t}{/if}{/each}</p>
    {/if}
  {:else}
    <p class="muted">No release notes.</p>
  {/each}
</div>

<style>
  .rn { display: flex; flex-direction: column; gap: 6px; font-size: 13.5px; }
  .h { font-family: var(--font-display); font-weight: 600; font-size: 13px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-text); margin-top: 4px; }
  ul { margin: 0; padding-left: 18px; display: flex; flex-direction: column; gap: 3px; }
  p { margin: 0; }
</style>
