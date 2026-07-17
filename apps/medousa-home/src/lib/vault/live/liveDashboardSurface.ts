/**
 * Liquid Live dashboard — title + columns on the page; Configure for metric tiles.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  parseDashboardFenceBody,
  serializeDashboardFence,
  type LiquidDashboardDraft,
} from "$lib/utils/vaultLiquidFence";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

function dashboardBody(raw: string): string {
  const open = /^```dashboard[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type DashboardSurfaceHandles = {
  destroy: () => void;
};

export function mountDashboardSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): DashboardSurfaceHandles {
  let draft: LiquidDashboardDraft = parseDashboardFenceBody(dashboardBody(raw));
  let currentRaw = serializeDashboardFence(draft);

  const root = document.createElement("div");
  root.className = "vault-live-dashboard";
  root.contentEditable = "false";

  const head = document.createElement("div");
  head.className = "vault-live-dashboard__head";

  const title = document.createElement("p");
  title.className = "vault-live-dashboard__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.dataset.placeholder = "Title";
  title.textContent = draft.title;
  title.setAttribute("role", "heading");
  title.setAttribute("aria-level", "3");

  const meta = document.createElement("div");
  meta.className = "vault-live-dashboard__meta vault-live-quiet-chrome";

  const colGroup = document.createElement("div");
  colGroup.className = "vault-live-dashboard__cols";
  colGroup.setAttribute("role", "group");
  colGroup.setAttribute("aria-label", "Columns");

  const syncCols = () => {
    for (const el of colGroup.querySelectorAll<HTMLButtonElement>(
      ".vault-live-dashboard__col",
    )) {
      el.setAttribute(
        "aria-pressed",
        el.dataset.cols === draft.columns ? "true" : "false",
      );
    }
  };

  for (const cols of ["2", "3", "4"] as const) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "vault-live-dashboard__col";
    btn.dataset.cols = cols;
    btn.textContent = cols;
    btn.setAttribute("aria-pressed", draft.columns === cols ? "true" : "false");
    btn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (draft.columns === cols) return;
      draft = { ...draft, columns: cols };
      syncCols();
      currentRaw = serializeDashboardFence(draft);
      onChange(currentRaw);
      remountPlot();
    });
    colGroup.append(btn);
  }

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-dashboard__configure";
  configure.textContent = "tiles";
  configure.title = "Edit metric tiles";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "dashboard";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  meta.append(colGroup, configure);
  head.append(title, meta);

  const stage = document.createElement("div");
  stage.className = "vault-live-dashboard__stage";

  const remountPlot = () => {
    unmountLiquidFence(stage);
    stage.replaceChildren();
    mountLiquidFence(stage, currentRaw, liquidContext);
    queueMicrotask(() => {
      for (const el of stage.querySelectorAll<HTMLElement>(".liquid-dashboard-header")) {
        el.hidden = true;
      }
    });
  };

  const commitTitle = () => {
    const next = title.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
    if (next === draft.title) return;
    draft = { ...draft, title: next };
    currentRaw = serializeDashboardFence(draft);
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
