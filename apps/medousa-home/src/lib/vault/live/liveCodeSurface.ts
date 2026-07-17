/**
 * Liquid Live code — title + lang on the page; Configure for source.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  parseCodeFenceBody,
  serializeCodeFence,
  type LiquidCodeDraft,
} from "$lib/utils/vaultLiquidFence";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

function codeBody(raw: string): string {
  const open = /^```code[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type CodeSurfaceHandles = {
  destroy: () => void;
};

export function mountCodeSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): CodeSurfaceHandles {
  let draft: LiquidCodeDraft = parseCodeFenceBody(codeBody(raw));
  let currentRaw = serializeCodeFence(draft);

  const root = document.createElement("div");
  root.className = "vault-live-code";
  root.contentEditable = "false";

  const head = document.createElement("div");
  head.className = "vault-live-code__head";

  const title = document.createElement("p");
  title.className = "vault-live-code__title";
  title.contentEditable = "true";
  title.spellcheck = false;
  title.dataset.placeholder = "Filename";
  title.textContent = draft.title;

  const meta = document.createElement("div");
  meta.className = "vault-live-code__meta vault-live-quiet-chrome";

  const lang = document.createElement("button");
  lang.type = "button";
  lang.className = "vault-live-code__lang";
  lang.textContent = draft.lang || "text";
  lang.title = "Language";
  lang.dataset.liveLiquidConfigure = "1";
  lang.dataset.liveLiquidLang = "code";
  lang.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-code__configure";
  configure.textContent = "source";
  configure.title = "Edit source";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "code";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  meta.append(lang, configure);
  head.append(title, meta);

  const stage = document.createElement("div");
  stage.className = "vault-live-code__stage";

  const remountPlot = () => {
    unmountLiquidFence(stage);
    stage.replaceChildren();
    mountLiquidFence(stage, currentRaw, liquidContext);
    queueMicrotask(() => {
      for (const el of stage.querySelectorAll<HTMLElement>(".liquid-code-header")) {
        el.hidden = true;
      }
    });
  };

  const commitTitle = () => {
    const next = title.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
    if (next === draft.title) return;
    draft = { ...draft, title: next };
    currentRaw = serializeCodeFence(draft);
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
