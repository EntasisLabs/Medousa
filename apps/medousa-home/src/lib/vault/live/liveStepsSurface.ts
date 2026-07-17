/**
 * Liquid Live steps — title on the page; Configure for step list / status.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  parseStepsFenceBody,
  serializeStepsFence,
  type LiquidStepsDraft,
} from "$lib/utils/vaultLiquidFence";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

function stepsBody(raw: string): string {
  const open = /^```steps[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type StepsSurfaceHandles = {
  destroy: () => void;
};

export function mountStepsSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): StepsSurfaceHandles {
  let draft: LiquidStepsDraft = parseStepsFenceBody(stepsBody(raw));
  let currentRaw = serializeStepsFence(draft);

  const root = document.createElement("div");
  root.className = "vault-live-steps";
  root.contentEditable = "false";

  const head = document.createElement("div");
  head.className = "vault-live-steps__head";

  const title = document.createElement("p");
  title.className = "vault-live-steps__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.dataset.placeholder = "Title";
  title.textContent = draft.title;
  title.setAttribute("role", "heading");
  title.setAttribute("aria-level", "3");

  const meta = document.createElement("div");
  meta.className = "vault-live-steps__meta vault-live-quiet-chrome";

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-steps__configure";
  configure.textContent = "steps";
  configure.title = "Edit steps";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "steps";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  meta.append(configure);
  head.append(title, meta);

  const stage = document.createElement("div");
  stage.className = "vault-live-steps__stage";

  const remountPlot = () => {
    unmountLiquidFence(stage);
    stage.replaceChildren();
    mountLiquidFence(stage, currentRaw, liquidContext);
    queueMicrotask(() => {
      for (const el of stage.querySelectorAll<HTMLElement>(".liquid-steps-header")) {
        el.hidden = true;
      }
    });
  };

  const commitTitle = () => {
    const next = title.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
    if (next === draft.title) return;
    draft = { ...draft, title: next };
    currentRaw = serializeStepsFence(draft);
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
