/**
 * Liquid Live tabs — title + active tab on the page; Configure for panel list.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  parseTabsFenceBody,
  serializeTabsFence,
  type LiquidTabsDraft,
} from "$lib/utils/vaultLiquidFence";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

function tabsBody(raw: string): string {
  const open = /^```tabs[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type TabsSurfaceHandles = {
  destroy: () => void;
};

export function mountTabsSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): TabsSurfaceHandles {
  let draft: LiquidTabsDraft = parseTabsFenceBody(tabsBody(raw));
  let currentRaw = serializeTabsFence(draft);

  const root = document.createElement("div");
  root.className = "vault-live-tabs";
  root.contentEditable = "false";

  const head = document.createElement("div");
  head.className = "vault-live-tabs__head";

  const title = document.createElement("p");
  title.className = "vault-live-tabs__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.dataset.placeholder = "Title";
  title.textContent = draft.title;
  title.setAttribute("role", "heading");
  title.setAttribute("aria-level", "3");

  const tabStrip = document.createElement("div");
  tabStrip.className = "vault-live-tabs__strip";
  tabStrip.setAttribute("role", "tablist");
  tabStrip.setAttribute("aria-label", "Tabs");

  const meta = document.createElement("div");
  meta.className = "vault-live-tabs__meta vault-live-quiet-chrome";

  const syncStrip = () => {
    tabStrip.replaceChildren();
    const active =
      draft.defaultLabel.trim() || draft.panels[0]?.label.trim() || "";
    for (const panel of draft.panels) {
      const label = panel.label.trim();
      if (!label) continue;
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "vault-live-tabs__tab";
      btn.textContent = label;
      btn.setAttribute("role", "tab");
      btn.setAttribute("aria-selected", label === active ? "true" : "false");
      btn.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      btn.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        if (draft.defaultLabel === label) return;
        draft = { ...draft, defaultLabel: label };
        currentRaw = serializeTabsFence(draft);
        onChange(currentRaw);
        syncStrip();
        remountPlot();
      });
      tabStrip.append(btn);
    }
  };

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-tabs__configure";
  configure.textContent = "panels";
  configure.title = "Edit tab panels";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "tabs";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  meta.append(configure);
  head.append(title, tabStrip, meta);

  const stage = document.createElement("div");
  stage.className = "vault-live-tabs__stage";

  const remountPlot = () => {
    unmountLiquidFence(stage);
    stage.replaceChildren();
    mountLiquidFence(stage, currentRaw, liquidContext);
    queueMicrotask(() => {
      for (const el of stage.querySelectorAll<HTMLElement>(".liquid-tabs-title")) {
        el.hidden = true;
      }
      // Live strip above owns tab switching; hide the organism duplicate.
      for (const el of stage.querySelectorAll<HTMLElement>(".liquid-tabs-list")) {
        el.hidden = true;
      }
    });
  };

  const commitTitle = () => {
    const next = title.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
    if (next === draft.title) return;
    draft = { ...draft, title: next };
    currentRaw = serializeTabsFence(draft);
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
  syncStrip();
  remountPlot();

  return {
    destroy: () => {
      unmountLiquidFence(stage);
      host.replaceChildren();
    },
  };
}
