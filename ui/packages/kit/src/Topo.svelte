<script lang="ts">
  // Topographic contour-line backdrop (original artwork, tools/topo). Place
  // inside a positioned container; it fills it behind the content.
  let {
    opacity = 0.09,
    tone = "accent",
    fade = "right",
    fixed = false,
    coords = "",
    scale = 1,
  }: {
    opacity?: number;
    tone?: "accent" | "fg";
    /** Where the pattern is strongest; it fades out towards the other side. */
    fade?: "right" | "left" | "top" | "center" | "none";
    /** Stay put while the page scrolls (app backgrounds). */
    fixed?: boolean;
    /** Map-style grid reference in the corner, e.g. "E 041 · N 118". */
    coords?: string;
    scale?: number;
  } = $props();
</script>

<div class="topo {tone} fade-{fade}" class:fixed style:--o={opacity} style:--s="{640 * scale}px" aria-hidden="true">
  {#if coords}<span class="coords">{coords}</span>{/if}
</div>

<style>
  .topo { position: absolute; inset: 0; z-index: 0; pointer-events: none; border-radius: inherit; overflow: hidden; }
  .topo.fixed { position: fixed; }
  .topo::before {
    content: ""; position: absolute; inset: 0; opacity: var(--o);
    background: var(--accent);
    -webkit-mask-image: url("./topo.svg"), var(--fade-mask, linear-gradient(#000, #000));
    -webkit-mask-size: var(--s) var(--s), 100% 100%;
    -webkit-mask-repeat: repeat, no-repeat;
    -webkit-mask-composite: source-in;
    mask-image: url("./topo.svg"), var(--fade-mask, linear-gradient(#000, #000));
    mask-size: var(--s) var(--s), 100% 100%;
    mask-repeat: repeat, no-repeat;
    mask-composite: intersect;
  }
  .topo.fg::before { background: var(--fg); }
  .fade-right { --fade-mask: radial-gradient(ellipse at 100% 0%, #000 0%, rgb(0 0 0 / 0.5) 40%, transparent 75%); }
  .fade-left { --fade-mask: radial-gradient(ellipse at 0% 0%, #000 0%, rgb(0 0 0 / 0.5) 40%, transparent 75%); }
  .fade-top { --fade-mask: linear-gradient(#000, transparent 85%); }
  .fade-center { --fade-mask: radial-gradient(ellipse at 50% 40%, #000 0%, rgb(0 0 0 / 0.4) 45%, transparent 80%); }
  .coords {
    position: absolute; right: 14px; top: 10px; font-family: var(--mono); font-size: 10.5px; letter-spacing: 0.12em;
    color: var(--accent-text); opacity: 0.55; text-transform: uppercase;
  }
  @media (max-width: 640px) { .coords { display: none; } }
</style>
