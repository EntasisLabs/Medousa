/**
 * Hydrate Liquid markdown placeholders into mounted Svelte components.
 *
 * Placeholders come from preprocessLiquidEmbeds:
 *   <div class="liquid-md-embed" data-liquid-embed="card" data-liquid-props="…">
 *   <span class="liquid-md-icon" data-liquid-icon="sparkles">
 */

import { mount, unmount, type Component } from "svelte";
import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  decodeLiquidProps,
  type LiquidEmbedKind,
} from "./liquidEmbeds";
import LiquidMdHost from "./LiquidMdHost.svelte";

type MountHandle = { destroy: () => void };

const mounts = new WeakMap<HTMLElement, MountHandle[]>();

function clearMounts(root: HTMLElement): void {
  const existing = mounts.get(root);
  if (!existing) return;
  for (const handle of existing) {
    try {
      handle.destroy();
    } catch {
      /* already gone */
    }
  }
  mounts.delete(root);
}

function mountHost(
  target: HTMLElement,
  kind: LiquidEmbedKind | "icon",
  payload: unknown,
  context: LiquidRenderContext,
): MountHandle {
  target.replaceChildren();
  const instance = mount(LiquidMdHost as unknown as Component, {
    target,
    props: { kind, payload, context },
  });
  return {
    destroy: () => {
      void unmount(instance);
    },
  };
}

/** Mount Liquid embeds inside a markdown container. Idempotent per content pass. */
export function hydrateLiquidEmbeds(
  root: HTMLElement,
  context: LiquidRenderContext = {},
): void {
  if (typeof window === "undefined") return;

  clearMounts(root);
  const handles: MountHandle[] = [];

  const embeds = root.querySelectorAll<HTMLElement>("[data-liquid-embed]");
  for (const el of embeds) {
    if (el.dataset.liquidHydrated === "1") continue;
    const kind = el.dataset.liquidEmbed as LiquidEmbedKind | undefined;
    const encoded = el.dataset.liquidProps;
    if (!kind || !encoded) continue;
    const payload = decodeLiquidProps(encoded);
    if (payload == null) continue;
    el.dataset.liquidHydrated = "1";
    handles.push(mountHost(el, kind, payload, context));
  }

  const icons = root.querySelectorAll<HTMLElement>("[data-liquid-icon]");
  for (const el of icons) {
    if (el.dataset.liquidHydrated === "1") continue;
    const id = el.dataset.liquidIcon?.trim();
    if (!id) continue;
    el.dataset.liquidHydrated = "1";
    handles.push(mountHost(el, "icon", id, context));
  }

  if (handles.length > 0) {
    mounts.set(root, handles);
  }
}

/** Tear down mounts for a container (e.g. before re-render). */
export function destroyLiquidEmbeds(root: HTMLElement): void {
  clearMounts(root);
}
