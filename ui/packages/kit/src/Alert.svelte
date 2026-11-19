<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import { CircleAlert, CircleCheck, Info, TriangleAlert, Sparkles, X } from "@lucide/svelte";

  let {
    tone = "info",
    title,
    icon,
    dismissible = false,
    ondismiss,
    actions,
    children,
  }: {
    tone?: "info" | "ok" | "warn" | "err" | "accent";
    title?: string;
    icon?: Component<any>;
    dismissible?: boolean;
    ondismiss?: () => void;
    actions?: Snippet;
    children?: Snippet;
  } = $props();

  const icons = { info: Info, ok: CircleCheck, warn: TriangleAlert, err: CircleAlert, accent: Sparkles };
  const Icon = $derived(icon ?? icons[tone]);
  let hidden = $state(false);
</script>

{#if !hidden}
  <div class="alert {tone}" role={tone === "err" ? "alert" : "status"}>
    <Icon size={17} />
    <div class="grow">
      {#if title}<b>{title}</b>{/if}
      {#if children}<div class="body">{@render children()}</div>{/if}
    </div>
    {#if actions}<div class="row acts">{@render actions()}</div>{/if}
    {#if dismissible}
      <button class="btn ghost icon sm" aria-label="Dismiss" onclick={() => ((hidden = true), ondismiss?.())}><X size={14} /></button>
    {/if}
  </div>
{/if}

<style>
  .alert { align-items: flex-start; }
  .body { font-size: 13.5px; color: var(--fg); }
  b { font-weight: 650; }
  b + .body { margin-top: 2px; color: var(--fg-2); }
  .acts { align-self: center; flex-shrink: 0; }
</style>
