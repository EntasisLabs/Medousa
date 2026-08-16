/**
 * Shared post-render hydration for markdown containers (chat, vault, PDF).
 *
 * Order matches chat MarkdownContent: destroy liquid → code → mermaid →
 * local images → liquid mounts. Mermaid and Liquid modules load only when
 * matching placeholders exist.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import { hydrateCodeBlocks } from "./codeBlocks";
import { hydrateLocalImages } from "./hydrateLocalImages";
import type { HydrateLiquidOptions } from "./hydrateLiquidEmbeds";
import { destroyDrawEmbeds, hydrateDrawEmbeds } from "$lib/draw/hydrateDrawEmbeds";

export interface HydrateMarkdownContainerOptions {
  liquidContext?: LiquidRenderContext;
  /** When set with localImages, resolve vault-relative image paths. */
  localImagePath?: string | null;
  code?: boolean;
  mermaid?: boolean;
  liquid?: boolean;
  localImages?: boolean;
  draw?: boolean;
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

type LiquidHydrateModule = typeof import("./hydrateLiquidEmbeds");
let liquidHydrate: LiquidHydrateModule | null = null;

async function liquidHydrateModule(): Promise<LiquidHydrateModule> {
  liquidHydrate ??= await import("./hydrateLiquidEmbeds");
  return liquidHydrate;
}

function hasLiquidPlaceholders(root: HTMLElement): boolean {
  return (
    root.querySelectorAll("[data-liquid-embed]").length > 0 ||
    root.querySelectorAll("[data-liquid-icon]").length > 0
  );
}

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
    draw = true,
    animate,
  } = options;

  if (liquid && (hasLiquidPlaceholders(root) || liquidHydrate)) {
    const { destroyLiquidEmbeds } = await liquidHydrateModule();
    destroyLiquidEmbeds(root);
  }
  if (draw) destroyDrawEmbeds(root);

  const tasks: Promise<unknown>[] = [];
  if (code) tasks.push(hydrateCodeBlocks(root));
  if (mermaid && root.querySelectorAll("pre.mermaid").length > 0) {
    const { hydrateMermaid } = await import("./mermaid");
    tasks.push(hydrateMermaid(root));
  }
  if (localImages) tasks.push(hydrateLocalImages(root, localImagePath));
  if (tasks.length) await Promise.all(tasks);

  if (draw) hydrateDrawEmbeds(root);

  if (liquid && hasLiquidPlaceholders(root)) {
    const { hydrateLiquidEmbeds } = await liquidHydrateModule();
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

export async function destroyMarkdownContainer(root: HTMLElement): Promise<void> {
  if (liquidHydrate) {
    liquidHydrate.destroyLiquidEmbeds(root);
  } else if (hasLiquidPlaceholders(root)) {
    const { destroyLiquidEmbeds } = await liquidHydrateModule();
    destroyLiquidEmbeds(root);
  }
  destroyDrawEmbeds(root);
}
