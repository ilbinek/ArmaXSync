<script lang="ts" generics="T">
  import type { Snippet } from "svelte";
  import { CircleAlert, RotateCw } from "@lucide/svelte";
  import Skeleton from "./Skeleton.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { errorText } from "./toast.svelte";

  let {
    load,
    children,
    loading,
    errorTitle = "Could not load",
  }: {
    load: () => Promise<T>;
    /** Receives the data and a reload function. */
    children: Snippet<[T, () => Promise<void>]>;
    loading?: Snippet;
    errorTitle?: string;
  } = $props();

  let data = $state<{ v: T } | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function run() {
    busy = true;
    error = null;
    try {
      data = { v: await load() };
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    load; // re-run when the loader changes
    run();
  });
</script>

{#if error && !data}
  <div class="card">
    <EmptyState icon={CircleAlert} title={errorTitle} description={error} compact>
      <button class="btn" onclick={run} disabled={busy}><RotateCw size={14} />Retry</button>
    </EmptyState>
  </div>
{:else if !data}
  {#if loading}{@render loading()}{:else}<div class="card"><Skeleton lines={4} height={16} /></div>{/if}
{:else}
  {@render children(data.v, run)}
{/if}
