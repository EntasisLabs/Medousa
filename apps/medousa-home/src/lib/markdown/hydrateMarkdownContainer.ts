/**
 * Shared post-render hydration for markdown containers (chat, vault, PDF).
 *
 * Order matches chat MarkdownContent: destroy liquid → code → mermaid →
 * local images → liquid mounts.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import { hydrateCodeBlocks } from "./codeBlocks";
import { hydrateLocalImages } from "./hydrateLocalImages";
import {
  destroyLiquidEmbeds,
  hydrateLiquidEmbeds,
  type HydrateLiquidOptions,
} from "./hydrateLiquidEmbeds";
import { hydrateMermaid } from "./mermaid";

export interface HydrateMarkdownContainerOptions {
  liquidContext?: LiquidRenderContext;
  /** When set with localImages, resolve vault-relative image paths. */
  localImagePath?: string | null;
  code?: boolean;
  mermaid?: boolean;
  liquid?: boolean;
  localImages?: boolean;
  /** Forwarded to liquid mounts (default true). */
  animate?: boolean;
}

/** Fingerprint of liquid placeholders — used to skip enter animation on remount. */
export function liquidPlaceholderFingerprint(root: HTMLElement): string {
  const embeds = [...root.querySelectorAll<HTMLElement>("[data-liquid-embed]")].map(
    (el) => `${el.dataset.liquidEmbed ?? ""}:${el.dataset.liquidProps ?? ""}`,
  );
  const icons = [...root.querySelectorAll<HTMLElement>("[data-liquid-icon]")].map(
    (el) => `icon:${el.dataset.liquidIcon ?? ""}`,
  );
  return [...embeds, ...icons].join("|");
}

const lastFingerprint = new WeakMap<HTMLElement, string>();

/**
 * Hydrate interactive pieces inside a rendered markdown root.
 * Returns a promise that settles after async code/mermaid/image work.
 */
export async function hydrateMarkdownContainer(
  root: HTMLElement,
  options: HydrateMarkdownContainerOptions = {},
): Promise<void> {
  if (typeof window === "undefined") return;

  const {
    liquidContext = {},
    localImagePath = null,
    code = true,
    mermaid = true,
    liquid = true,
    localImages = false,
    animate,
  } = options;

  if (liquid) destroyLiquidEmbeds(root);

  const tasks: Promise<unknown>[] = [];
  if (code) tasks.push(hydrateCodeBlocks(root));
  if (mermaid) tasks.push(hydrateMermaid(root));
  if (localImages) tasks.push(hydrateLocalImages(root, localImagePath));
  if (tasks.length) await Promise.all(tasks);

  if (liquid) {
    const fingerprint = liquidPlaceholderFingerprint(root);
    const unchanged = lastFingerprint.get(root) === fingerprint && fingerprint.length > 0;
    lastFingerprint.set(root, fingerprint);
    const liquidOpts: HydrateLiquidOptions = {
      context: liquidContext,
      animate: animate ?? !unchanged,
    };
    hydrateLiquidEmbeds(root, liquidOpts);
  }
}
