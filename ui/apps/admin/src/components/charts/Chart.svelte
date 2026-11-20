<script lang="ts">
  import { onMount } from "svelte";
  import { theme } from "@axs/kit";
  import { echarts, styled, type EChartsOption } from "./echarts";

  let { option, height = 260, label = "" }: { option: EChartsOption; height?: number; label?: string } = $props();

  let el: HTMLDivElement;
  let chart: echarts.ECharts | null = null;

  onMount(() => {
    chart = echarts.init(el, undefined, { renderer: "canvas" });
    const ro = new ResizeObserver(() => chart?.resize());
    ro.observe(el);
    const mq = matchMedia("(prefers-color-scheme: light)");
    const onScheme = () => chart?.setOption(styled($state.snapshot(option) as EChartsOption), { notMerge: true });
    mq.addEventListener("change", onScheme);
    return () => {
      ro.disconnect();
      mq.removeEventListener("change", onScheme);
      chart?.dispose();
      chart = null;
    };
  });

  // Re-render when the data or the theme changes.
  $effect(() => {
    theme.value;
    const o = $state.snapshot(option) as EChartsOption;
    if (!chart) return;
    // Theme tokens are read after the DOM attribute flips.
    requestAnimationFrame(() => chart?.setOption(styled(o), { notMerge: true }));
  });
</script>

<div class="chart" bind:this={el} style:height="{height}px" role="img" aria-label={label}></div>

<style>
  .chart { width: 100%; min-width: 0; }
</style>
