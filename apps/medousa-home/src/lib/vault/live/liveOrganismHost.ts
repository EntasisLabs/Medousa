/**
 * Mount Preview-class liquid organisms into Live TipTap NodeView DOM.
 */

import {
  destroyLiquidEmbeds,
  hydrateLiquidEmbeds,
} from "$lib/markdown/hydrateLiquidEmbeds";
import { preprocessLiquidEmbeds, LIQUID_FENCE_LANGS } from "$lib/markdown/liquidEmbeds";
import type { LiquidRenderContext } from "$lib/liquid/render/context";
import { highlightElement } from "$lib/syntax/highlightCode";

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

/** Plain (non-liquid) fence: calm monospace body + quiet corner edit. */
export function mountPlainFence(
  host: HTMLElement,
  lang: string,
  body: string,
  onEditRaw?: () => void,
): void {
  host.replaceChildren();
  const card = document.createElement("div");
  card.className = "vault-live-plain-fence";

  const head = document.createElement("div");
  head.className = "vault-live-plain-fence__head";

  const label = document.createElement("span");
  label.className = "vault-live-plain-fence__lang";
  label.textContent = lang || "code";

  head.append(label);

  if (onEditRaw) {
    const meta = document.createElement("div");
    meta.className = "vault-live-plain-fence__meta vault-live-quiet-chrome";

    const edit = document.createElement("button");
    edit.type = "button";
    edit.className = "vault-live-plain-fence__edit";
    edit.textContent = "edit";
    edit.title = "Edit fence source";
    edit.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    edit.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      onEditRaw();
    });

    meta.append(edit);
    head.append(meta);
  }

  const pre = document.createElement("pre");
  pre.className = "vault-live-plain-fence__body";

  const code = document.createElement("code");
  const langId = lang.trim().toLowerCase() || "plaintext";
  code.className = `syn-code language-${langId}`;
  code.textContent = body;
  pre.append(code);

  card.append(head, pre);
  host.append(card);

  void highlightElement(code, langId);
}
