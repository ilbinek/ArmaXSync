// ECharts, registered modularly (only what the admin uses), styled from the
// design tokens so charts follow the dark/light theme.
import * as echarts from "echarts/core";
import { LineChart, BarChart, HeatmapChart, PieChart } from "echarts/charts";
import { GridComponent, TooltipComponent, LegendComponent, DataZoomComponent, VisualMapComponent, MarkLineComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import type { EChartsOption } from "echarts";
import { bytes } from "@axs/kit";

echarts.use([LineChart, BarChart, HeatmapChart, PieChart, GridComponent, TooltipComponent, LegendComponent, DataZoomComponent, VisualMapComponent, MarkLineComponent, CanvasRenderer]);

export { echarts };
export type { EChartsOption };

function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Categorical palette: accent first, then distinguishable hues. */
export function palette(): string[] {
  const light = document.documentElement.dataset.theme === "light" || (document.documentElement.dataset.theme === "system" && matchMedia("(prefers-color-scheme: light)").matches);
  return [
    token("--accent"),
    token("--info"),
    token("--ok"),
    light ? "#7c3aed" : "#a78bfa",
    light ? "#0f766e" : "#2dd4bf",
    token("--warn"),
    light ? "#be185d" : "#f472b6",
    token("--fg-3"),
  ];
}

export const fmt = {
  bytes: (v: number) => bytes(v),
  speed: (v: number) => `${bytes(v)}/s`,
  num: (v: number) => (v >= 10_000 ? `${(v / 1000).toFixed(v >= 100_000 ? 0 : 1)}k` : Math.round(v).toLocaleString()),
  pct: (v: number) => `${(v * 100).toFixed(1)}%`,
  ms: (v: number) => (v >= 60_000 ? `${(v / 60_000).toFixed(1)} min` : v >= 1000 ? `${(v / 1000).toFixed(1)} s` : `${Math.round(v)} ms`),
};

/** Theme defaults merged under every chart option. */
export function styled(opt: EChartsOption): EChartsOption {
  const fg2 = token("--fg-2");
  const fg3 = token("--fg-3");
  const line = token("--line");
  const surface = token("--surface-2");
  const strong = token("--line-strong");
  const font = token("--font") || "system-ui";
  const axis = (a: any) =>
    a == null
      ? a
      : (Array.isArray(a) ? a : [a]).map((x: any) => ({
          axisLine: { lineStyle: { color: strong } },
          axisTick: { show: false },
          axisLabel: { color: fg3, fontSize: 11, hideOverlap: true, ...(x.axisLabel ?? {}) },
          splitLine: { lineStyle: { color: line, type: [3, 4] }, ...(x.splitLine ?? {}) },
          ...x,
          ...(x.axisLabel ? { axisLabel: { color: fg3, fontSize: 11, hideOverlap: true, ...x.axisLabel } } : {}),
        }));
  return {
    backgroundColor: "transparent",
    color: palette(),
    textStyle: { fontFamily: font, color: fg2 },
    animationDuration: 600,
    animationEasing: "cubicOut",
    grid: { left: 8, right: 12, top: 28, bottom: 8, containLabel: true },
    ...opt,
    tooltip: opt.tooltip
      ? {
          backgroundColor: surface,
          borderColor: strong,
          borderWidth: 1,
          textStyle: { color: token("--fg"), fontSize: 12.5, fontFamily: font },
          extraCssText: "box-shadow: 0 10px 30px rgba(0,0,0,.35); border-radius: 6px;",
          ...(opt.tooltip as object),
        }
      : undefined,
    legend: opt.legend
      ? { textStyle: { color: fg2, fontSize: 12 }, icon: "roundRect", itemWidth: 10, itemHeight: 10, top: 0, right: 0, ...(opt.legend as object) }
      : undefined,
    xAxis: axis(opt.xAxis) as any,
    yAxis: axis(opt.yAxis) as any,
  };
}

/** Value axis with a formatter. */
export const valueAxis = (format: (v: number) => string = fmt.num, extra: object = {}) => ({
  type: "value" as const,
  axisLabel: { formatter: (v: number) => format(v) },
  ...extra,
});

/** Tooltip listing every series with a formatter. */
export const axisTooltip = (format: (v: number) => string, title?: (p: any) => string) => ({
  trigger: "axis" as const,
  axisPointer: { type: "line" as const, lineStyle: { color: token("--line-strong") } },
  formatter: (ps: any) => {
    const arr = Array.isArray(ps) ? ps : [ps];
    const head = title ? title(arr[0]) : arr[0]?.axisValueLabel ?? "";
    const rows = arr
      .filter((p: any) => p.value != null)
      .map((p: any) => {
        const v = Array.isArray(p.value) ? p.value[1] : p.value;
        return `<div style="display:flex;gap:10px;justify-content:space-between"><span>${p.marker}${p.seriesName}</span><b>${format(Number(v))}</b></div>`;
      })
      .join("");
    return `<div style="margin-bottom:4px;opacity:.75">${head}</div>${rows}`;
  },
});

/** Area series style for time series (thin lines when stacked). */
export const area = (color?: string, opacity = 0.22, stacked = false) => ({
  type: "line" as const,
  smooth: 0.25,
  showSymbol: false,
  lineStyle: { width: stacked ? 1 : 2 },
  areaStyle: {
    opacity: 1,
    color: color
      ? new echarts.graphic.LinearGradient(0, 0, 0, 1, [
          { offset: 0, color: hexA(color, opacity) },
          { offset: 1, color: hexA(color, 0) },
        ])
      : undefined,
  },
});

/** "#rrggbb" or "rgb(…)" with alpha. */
export function hexA(c: string, a: number): string {
  if (c.startsWith("#") && c.length === 7) {
    const n = parseInt(c.slice(1), 16);
    return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
  }
  return c;
}

export const dayLabel = (d: string) => new Date(d + "T00:00:00Z").toLocaleDateString(undefined, { month: "short", day: "numeric", timeZone: "UTC" });
export const hourLabel = (t: number) => new Date(t).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
