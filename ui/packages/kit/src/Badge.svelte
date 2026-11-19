<script lang="ts">
  import type { Component, Snippet } from "svelte";
  let {
    tone = "neutral",
    dot = false,
    pulse = false,
    plain = false,
    icon: Icon,
    title,
    children,
  }: {
    tone?: "neutral" | "ok" | "warn" | "err" | "info" | "accent";
    dot?: boolean;
    pulse?: boolean;
    plain?: boolean;
    icon?: Component<any>;
    title?: string;
    children: Snippet;
  } = $props();
</script>

<span class="badge {tone === 'neutral' ? '' : tone}" class:plain {title}>
  {#if dot}<span class="dot {tone}" class:pulse></span>{:else if Icon}<Icon size={12} />{/if}
  {@render children()}
</span>

<style>
  .badge .dot { width: 6px; height: 6px; box-shadow: none; }
</style>
