/**
 * Liquid Live timeline — title + layout on the page; Configure for events.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  parseTimelineFenceBody,
  serializeTimelineFence,
  type LiquidTimelineDraft,
  type LiquidTimelineLayout,
} from "$lib/utils/vaultLiquidFence";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

function timelineBody(raw: string): string {
  const open = /^```timeline[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type TimelineSurfaceHandles = {
  destroy: () => void;
};

export function mountTimelineSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): TimelineSurfaceHandles {
  let draft: LiquidTimelineDraft = parseTimelineFenceBody(timelineBody(raw));
  let currentRaw = serializeTimelineFence(draft);

  const root = document.createElement("div");
  root.className = "vault-live-timeline";
  root.contentEditable = "false";

  const head = document.createElement("div");
  head.className = "vault-live-timeline__head";

  const title = document.createElement("p");
  title.className = "vault-live-timeline__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.dataset.placeholder = "Title";
  title.textContent = draft.title;
  title.setAttribute("role", "heading");
  title.setAttribute("aria-level", "3");

  const meta = document.createElement("div");
  meta.className = "vault-live-timeline__meta vault-live-quiet-chrome";

  const layoutGroup = document.createElement("div");
  layoutGroup.className = "vault-live-timeline__layouts";
  layoutGroup.setAttribute("role", "group");
  layoutGroup.setAttribute("aria-label", "Layout");

  const syncLayouts = () => {
    for (const el of layoutGroup.querySelectorAll<HTMLButtonElement>(
      ".vault-live-timeline__layout",
    )) {
      el.setAttribute(
        "aria-pressed",
        el.dataset.layout === draft.layout ? "true" : "false",
      );
    }
  };

  const layouts: LiquidTimelineLayout[] = ["rail", "snapshot"];
  for (const layout of layouts) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "vault-live-timeline__layout";
    btn.dataset.layout = layout;
    btn.textContent = layout;
    btn.setAttribute("aria-pressed", draft.layout === layout ? "true" : "false");
    btn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (draft.layout === layout) return;
      draft = { ...draft, layout };
      syncLayouts();
      currentRaw = serializeTimelineFence(draft);
      onChange(currentRaw);
      remountPlot();
    });
    layoutGroup.append(btn);
  }

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-timeline__configure";
  configure.textContent = "timeline";
  configure.title = "Edit timeline events";
  configure.dataset.liveLiquidConfigure = "1";
  configure.dataset.liveLiquidLang = "timeline";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  meta.append(layoutGroup, configure);
  head.append(title, meta);

  const stage = document.createElement("div");
  stage.className = "vault-live-timeline__stage";

  const remountPlot = () => {
    unmountLiquidFence(stage);
    stage.replaceChildren();
    mountLiquidFence(stage, currentRaw, liquidContext);
    queueMicrotask(() => {
      for (const el of stage.querySelectorAll<HTMLElement>(
        ".liquid-timeline-header, .liquid-timeline-snapshot-header",
      )) {
        el.hidden = true;
      }
    });
  };

  const commitTitle = () => {
    const next = title.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
    if (next === draft.title) return;
    draft = { ...draft, title: next };
    currentRaw = serializeTimelineFence(draft);
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
