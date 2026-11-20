import type { Component } from "svelte";

export type Column<R> = {
  key: string;
  label: string;
  /** Value used for sorting (enables the sort button). */
  sort?: (r: R) => string | number | null | undefined;
  align?: "left" | "right" | "center";
  width?: string;
  class?: string;
  /** Hide on narrow screens. */
  hideSm?: boolean;
};

export type MenuItem =
  | { label: string; icon?: Component<any>; onclick: () => void; danger?: boolean; disabled?: boolean; hint?: string }
  | { divider: true };

export type TabItem = { id: string; label: string; icon?: Component<any>; count?: number | string | null; hidden?: boolean };

export type DateRange = { from: number; to: number; preset: string };

export type PaletteItem = { id: string; label: string; group: string; icon?: Component<any>; hint?: string; keywords?: string; /** Ranking bias (actions > pages > history). */ weight?: number; run: () => void };
