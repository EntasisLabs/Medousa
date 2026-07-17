/**
 * Liquid Live tree — title on the page; Configure for indented tree text.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  parseTreeFenceBody,
  serializeTreeFence,
  type LiquidTreeDraft,
} from "$lib/utils/vaultLiquidFence";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

function treeBody(raw: string): string {
  const open = /^```tree[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type TreeSurfaceHandles = {
  destroy: () => void;
};

export function mountTreeSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): TreeSurfaceHandles {
  let draft: LiquidTreeDraft = parseTreeFenceBody(treeBody(raw));
  let currentRaw = serializeTreeFence(draft);

  const root = document.createElement("div");
  root.className = "vault-live-tree";
  root.contentEditable = "false";

  const head = document.createElement("div");
  head.className = "vault-live-tree__head";

  const title = document.createElement("p");
  title.className = "vault-live-tree__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.dataset.placeholder = "Title";
  title.textContent = draft.title;
  title.setAttribute("role", "heading");
  title.setAttribute("aria-level", "3");

  const meta = document.createElement("div");
  meta.className = "vault-live-tree__meta vault-live-quiet-chrome";

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-tree__configure";
  configure.textContent = "tree";
  configure.title = "Edit tree";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "tree";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  meta.append(configure);
  head.append(title, meta);

  const stage = document.createElement("div");
  stage.className = "vault-live-tree__stage";

  const remountPlot = () => {
    unmountLiquidFence(stage);
    stage.replaceChildren();
    mountLiquidFence(stage, currentRaw, liquidContext);
    queueMicrotask(() => {
      for (const el of stage.querySelectorAll<HTMLElement>(
        ".liquid-tree-header, .liquid-tree-title",
      )) {
        el.hidden = true;
      }
    });
  };

  const commitTitle = () => {
    const next = title.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
    if (next === draft.title) return;
    draft = { ...draft, title: next };
    currentRaw = serializeTreeFence(draft);
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
