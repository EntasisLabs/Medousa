/**
 * Liquid Live compare — title + mode on the page; Configure for the table.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  parseCompareFenceBody,
  serializeCompareFence,
  type LiquidCompareDraft,
  type LiquidCompareMode,
} from "$lib/utils/vaultLiquidFence";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

function compareBody(raw: string): string {
  const open = /^```compare[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type CompareSurfaceHandles = {
  destroy: () => void;
};

export function mountCompareSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): CompareSurfaceHandles {
  let draft: LiquidCompareDraft = parseCompareFenceBody(compareBody(raw));
  let currentRaw = serializeCompareFence(draft);

  const root = document.createElement("div");
  root.className = "vault-live-compare";
  root.contentEditable = "false";

  const head = document.createElement("div");
  head.className = "vault-live-compare__head";

  const title = document.createElement("p");
  title.className = "vault-live-compare__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.dataset.placeholder = "Title";
  title.textContent = draft.title;
  title.setAttribute("role", "heading");
  title.setAttribute("aria-level", "3");

  const meta = document.createElement("div");
  meta.className = "vault-live-compare__meta vault-live-quiet-chrome";

  const modeGroup = document.createElement("div");
  modeGroup.className = "vault-live-compare__modes";
  modeGroup.setAttribute("role", "group");
  modeGroup.setAttribute("aria-label", "Presentation");

  const syncModes = () => {
    for (const el of modeGroup.querySelectorAll<HTMLButtonElement>(
      ".vault-live-compare__mode",
    )) {
      el.setAttribute(
        "aria-pressed",
        el.dataset.mode === draft.mode ? "true" : "false",
      );
    }
  };

  const modes: LiquidCompareMode[] = ["matrix", "faceoff"];
  for (const mode of modes) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "vault-live-compare__mode";
    btn.dataset.mode = mode;
    btn.textContent = mode === "faceoff" ? "face-off" : "matrix";
    btn.setAttribute("aria-pressed", draft.mode === mode ? "true" : "false");
    btn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (draft.mode === mode) return;
      draft = { ...draft, mode };
      syncModes();
      currentRaw = serializeCompareFence(draft);
      onChange(currentRaw);
      remountPlot();
    });
    modeGroup.append(btn);
  }

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-compare__configure";
  configure.textContent = "table";
  configure.title = "Edit compare table";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "compare";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  meta.append(modeGroup, configure);
  head.append(title, meta);

  const stage = document.createElement("div");
  stage.className = "vault-live-compare__stage";

  const remountPlot = () => {
    unmountLiquidFence(stage);
    stage.replaceChildren();
    mountLiquidFence(stage, currentRaw, liquidContext);
    queueMicrotask(() => {
      for (const el of stage.querySelectorAll<HTMLElement>(".liquid-compare-header")) {
        el.hidden = true;
      }
    });
  };

  const commitTitle = () => {
    const next = title.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
    if (next === draft.title) return;
    draft = { ...draft, title: next };
    currentRaw = serializeCompareFence(draft);
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
