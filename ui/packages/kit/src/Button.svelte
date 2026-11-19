<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  import { LoaderCircle } from "@lucide/svelte";

  type Props = Omit<HTMLButtonAttributes, "class"> & {
    variant?: "default" | "primary" | "danger" | "danger-solid" | "ghost" | "subtle";
    size?: "sm" | "md" | "lg";
    icon?: Component<any>;
    iconRight?: Component<any>;
    loading?: boolean;
    href?: string;
    target?: string;
    tip?: string;
    class?: string;
    children?: Snippet;
  };
  let {
    variant = "default",
    size = "md",
    icon: Icon,
    iconRight: IconRight,
    loading = false,
    href,
    target,
    tip,
    class: cls = "",
    disabled,
    type = "button",
    children,
    ...rest
  }: Props = $props();

  const classes = $derived(
    ["btn", variant === "danger-solid" ? "danger solid" : variant !== "default" ? variant : "", size !== "md" ? size : "", !children ? "icon" : "", cls]
      .filter(Boolean)
      .join(" "),
  );
  const iconSize = $derived(size === "sm" ? 14 : size === "lg" ? 18 : 16);
</script>

{#if href && !disabled}
  <a {href} {target} rel={target === "_blank" ? "noopener" : undefined} class={classes} data-tip={tip} aria-label={!children ? tip : undefined}>
    {#if Icon}<Icon size={iconSize} />{/if}
    {@render children?.()}
    {#if IconRight}<IconRight size={iconSize} />{/if}
  </a>
{:else}
  <button {type} class={classes} disabled={disabled || loading} aria-busy={loading} data-tip={tip} aria-label={!children ? tip : undefined} {...rest}>
    {#if loading}<LoaderCircle size={iconSize} class="spin" />{:else if Icon}<Icon size={iconSize} />{/if}
    {@render children?.()}
    {#if IconRight && !loading}<IconRight size={iconSize} />{/if}
  </button>
{/if}
