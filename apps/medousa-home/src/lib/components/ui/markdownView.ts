import type { Component } from "svelte";
import type { LiquidMarkdownViewProps } from "$lib/liquid/render/context";

export type MarkdownViewComponent = Component<LiquidMarkdownViewProps>;

let registered: MarkdownViewComponent | null = null;

export function setMarkdownViewComponent(component: MarkdownViewComponent | null): void {
  registered = component;
}

export function getMarkdownViewComponent(): MarkdownViewComponent | null {
  return registered;
}
