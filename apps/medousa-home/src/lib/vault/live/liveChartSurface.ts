/**
 * Liquid Live chart — one figure. Title + type live on the organism;
 * Configure is a whisper. Slash arrival picks type on the figure first.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  chartFenceTemplateForType,
  type ChartFenceType,
} from "$lib/utils/liquidFenceTemplates";
import {
  parseChartFenceParts,
  serializeChartFenceFromParts,
  type ChartFenceKv,
} from "$lib/utils/vaultChartFence";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";

const QUICK_TYPES: ChartFenceType[] = ["bar", "line", "area", "pie"];

/** Marker: slash just dropped this — pick type on the figure before it feels “wrong”. */
export const LIVE_CHART_ARRIVAL_KEY = "liveArrival";

export const LIQUID_CHART_ARRIVAL_TEMPLATE = [
  "```chart",
  "type: bar",
  "title: Chart",
  `${LIVE_CHART_ARRIVAL_KEY}: 1`,
  "",
  "| Month | Value |",
  "| --- | --- |",
  "| Jan | 12 |",
  "| Feb | 19 |",
  "| Mar | 14 |",
  "```",
  "",
].join("\n");

function chartBody(raw: string): string {
  const open = /^```chart[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

function isArrivalRaw(raw: string): boolean {
  return new RegExp(`^\\s*${LIVE_CHART_ARRIVAL_KEY}\\s*:\\s*1\\s*$`, "im").test(
    chartBody(raw),
  );
}

function patchFenceRaw(
  raw: string,
  kv: ChartFenceKv,
  options?: { clearArrival?: boolean },
): string {
  const parts = parseChartFenceParts(chartBody(raw));
  const preserved = { ...parts.allFields };
  if (options?.clearArrival) {
    delete preserved.livearrival;
    delete preserved[LIVE_CHART_ARRIVAL_KEY.toLowerCase()];
  }
  return (
    serializeChartFenceFromParts({ ...parts, allFields: preserved }, kv) + "\n"
  );
}

export type ChartSurfaceHandles = {
  destroy: () => void;
};

export function mountChartSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): ChartSurfaceHandles {
  let currentRaw = raw.endsWith("\n") ? raw : `${raw}\n`;
  let parts = parseChartFenceParts(chartBody(currentRaw));
  let kv: ChartFenceKv = { ...parts.kv };
  let arrival = isArrivalRaw(currentRaw);

  const root = document.createElement("div");
  root.className = "vault-live-chart";
  root.contentEditable = "false";
  if (arrival) root.classList.add("vault-live-chart--arrival");

  const head = document.createElement("div");
  head.className = "vault-live-chart__head";

  const title = document.createElement("p");
  title.className = "vault-live-chart__title";
  title.contentEditable = "true";
  title.spellcheck = true;
  title.dataset.placeholder = "Title";
  title.textContent = kv.title;
  title.setAttribute("role", "heading");
  title.setAttribute("aria-level", "3");

  const meta = document.createElement("div");
  meta.className = "vault-live-chart__meta vault-live-quiet-chrome";

  const typeGroup = document.createElement("div");
  typeGroup.className = "vault-live-chart__types";
  typeGroup.setAttribute("role", "group");
  typeGroup.setAttribute("aria-label", "Chart type");

  const syncTypePressed = () => {
    for (const el of typeGroup.querySelectorAll<HTMLButtonElement>(".vault-live-chart__type")) {
      el.setAttribute(
        "aria-pressed",
        el.dataset.type === kv.type ? "true" : "false",
      );
    }
  };

  const applyType = (type: ChartFenceType) => {
    if (arrival) {
      arrival = false;
      root.classList.remove("vault-live-chart--arrival");
      currentRaw = chartFenceTemplateForType(type);
      // Keep a gentle default title if template has one; focus title after.
      parts = parseChartFenceParts(chartBody(currentRaw));
      kv = { ...parts.kv };
      title.textContent = kv.title;
      syncTypePressed();
      onChange(currentRaw.endsWith("\n") ? currentRaw : `${currentRaw}\n`);
      return;
    }
    if (kv.type === type) return;
    kv = { ...kv, type };
    syncTypePressed();
    currentRaw = patchFenceRaw(currentRaw, kv, { clearArrival: true });
    onChange(currentRaw);
  };

  for (const type of QUICK_TYPES) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "vault-live-chart__type";
    btn.dataset.type = type;
    btn.textContent = type;
    btn.setAttribute("aria-pressed", type === kv.type ? "true" : "false");
    btn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      applyType(type);
    });
    typeGroup.append(btn);
  }

  const configure = document.createElement("button");
  configure.type = "button";
  configure.className = "vault-live-chart__configure";
  configure.textContent = "data";
  configure.title = "Configure data & style";
  configure.dataset.liveChartConfigure = "1";
  configure.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  meta.append(typeGroup, configure);

  const commitTitle = () => {
    const next = title.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
    if (next === kv.title) return;
    kv = { ...kv, title: next };
    if (arrival) {
      // Stay in arrival but remember title for after pick.
      currentRaw = patchFenceRaw(currentRaw, kv);
      onChange(currentRaw);
      return;
    }
    currentRaw = patchFenceRaw(currentRaw, kv);
    onChange(currentRaw);
  };
  title.addEventListener("blur", commitTitle);
  title.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      title.blur();
    }
  });

  head.append(title, meta);

  const stage = document.createElement("div");
  stage.className = "vault-live-chart__stage";

  const arrivalPane = document.createElement("div");
  arrivalPane.className = "vault-live-chart__arrival";
  arrivalPane.innerHTML = `<p class="vault-live-chart__arrival-label">What kind of chart?</p>`;
  const arrivalTypes = document.createElement("div");
  arrivalTypes.className = "vault-live-chart__arrival-types";
  for (const type of QUICK_TYPES) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "vault-live-chart__arrival-type";
    btn.textContent = type;
    btn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      applyType(type);
    });
    arrivalTypes.append(btn);
  }
  arrivalPane.append(arrivalTypes);

  const remountPlot = () => {
    unmountLiquidFence(stage);
    stage.replaceChildren();
    if (arrival) {
      stage.append(arrivalPane);
      return;
    }
    mountLiquidFence(stage, currentRaw, liquidContext);
    queueMicrotask(() => {
      for (const el of stage.querySelectorAll<HTMLElement>(".liquid-chart-header")) {
        el.hidden = true;
      }
      for (const el of stage.querySelectorAll(".liquid-chart-toolbar")) {
        el.remove();
      }
    });
  };

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
