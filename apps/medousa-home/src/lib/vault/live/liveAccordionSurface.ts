/**
 * Liquid Live accordion — title on the page; Configure for items.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  parseAccordionFenceBody,
  serializeAccordionFence,
  type LiquidAccordionDraft,
} from "$lib/utils/vaultLiquidFence";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

function accordionBody(raw: string): string {
  const open = /^```accordion[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type AccordionSurfaceHandles = {
  destroy: () => void;
};

export function mountAccordionSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): AccordionSurfaceHandles {
  let draft: LiquidAccordionDraft = parseAccordionFenceBody(accordionBody(raw));
  let currentRaw = serializeAccordionFence(draft);

  const root = document.createElement("div");
  root.className = "vault-live-accordion";
  root.contentEditable = "false";

  const head = document.createElement("div");
  head.className = "vault-live-accordion__head";

  const title = document.createElement("p");
  title.className = "vault-live-accordion__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.dataset.placeholder = "Title";
  title.textContent = draft.title;
  title.setAttribute("role", "heading");
  title.setAttribute("aria-level", "3");

  const meta = document.createElement("div");
  meta.className = "vault-live-accordion__meta vault-live-quiet-chrome";

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-accordion__configure";
  configure.textContent = "items";
  configure.title = "Edit accordion items";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "accordion";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  meta.append(configure);
  head.append(title, meta);

  const stage = document.createElement("div");
  stage.className = "vault-live-accordion__stage";

  const remountPlot = () => {
    unmountLiquidFence(stage);
    stage.replaceChildren();
    mountLiquidFence(stage, currentRaw, liquidContext);
    queueMicrotask(() => {
      for (const el of stage.querySelectorAll<HTMLElement>(
        ".liquid-accordion-header, .liquid-accordion-title",
      )) {
        el.hidden = true;
      }
    });
  };

  const commitTitle = () => {
    const next = title.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
    if (next === draft.title) return;
    draft = { ...draft, title: next };
    currentRaw = serializeAccordionFence(draft);
    onChange(currentRaw);
    remountPlot();
  };
  title.addEventListener("blur", commitTitle);
  title.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      title.blur();
    }
  });

  root.append(head, stage);
  host.replaceChildren(root);
  remountPlot();

  return {
    destroy: () => {
      unmountLiquidFence(stage);
      host.replaceChildren();
    },
  };
}
