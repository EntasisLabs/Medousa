/**
 * Mount Preview-class liquid organisms into Live TipTap NodeView DOM.
 */

import {
  destroyLiquidEmbeds,
  hydrateLiquidEmbeds,
} from "$lib/markdown/hydrateLiquidEmbeds";
import { preprocessLiquidEmbeds, LIQUID_FENCE_LANGS } from "$lib/markdown/liquidEmbeds";
import type { LiquidRenderContext } from "$lib/liquid/render/context";

export function isLiquidFenceLang(lang: string): boolean {
  return LIQUID_FENCE_LANGS.has(lang.toLowerCase());
}

export function mountLiquidFence(
  host: HTMLElement,
  raw: string,
  context: LiquidRenderContext = {},
): void {
  destroyLiquidEmbeds(host);
  host.replaceChildren();
  const wrap = document.createElement("div");
  wrap.className = "vault-live-organism markdown-content";
  wrap.innerHTML = preprocessLiquidEmbeds(raw);
  host.append(wrap);
  hydrateLiquidEmbeds(wrap, { context, animate: false });
}

export function unmountLiquidFence(host: HTMLElement): void {
  const wrap = host.querySelector(".vault-live-organism");
  if (wrap instanceof HTMLElement) {
    destroyLiquidEmbeds(wrap);
  }
  destroyLiquidEmbeds(host);
  host.replaceChildren();
}

/** Plain (non-liquid) fence: calm monospace body, no Build exile. */
export function mountPlainFence(
  host: HTMLElement,
  lang: string,
  body: string,
): void {
  host.replaceChildren();
  const card = document.createElement("div");
  card.className = "vault-live-plain-fence";
  const label = document.createElement("span");
  label.className = "vault-live-plain-fence__lang";
  label.textContent = lang || "code";
  const pre = document.createElement("pre");
  pre.className = "vault-live-plain-fence__body";
  pre.textContent = body;
  card.append(label, pre);
  host.append(card);
}
