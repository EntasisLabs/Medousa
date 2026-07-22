/**
 * Liquid Live slides — deck preview + quiet chrome for columns / write mode.
 */

import type { LiquidRenderContext } from "$lib/liquid/render/context";
import {
  parseSlidesDeck,
  serializeSlidesDeckBody,
  serializeSlidesFence,
  type SlidesDeck,
  type SlidesDeckWidth,
} from "$lib/utils/markdownSlides";
import { registerLiveDraftFlush } from "./liveDraftFlush";
import { mountLiquidFence, unmountLiquidFence } from "./liveOrganismHost";
import {
  whenElementHasLayout,
  type LayoutWaitHandle,
} from "./whenElementHasLayout";
import { LIVE_EMBED_WIDTHS, embedWidthClass } from "./liveEmbedWidth";

function fenceInner(raw: string): string {
  const open = /^```slides[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export function parseSlidesRaw(raw: string): SlidesDeck {
  return (
    parseSlidesDeck(fenceInner(raw)) ?? {
      title: "",
      theme: "paper",
      columns: "2",
      slides: [
        {
          id: "slide-1",
          label: "Title",
          layout: "hero",
          body: "# New slide\n",
        },
      ],
    }
  );
}

export function serializeSlidesRaw(model: SlidesDeck): string {
  return serializeSlidesFence(model);
}

export type SlidesSurfaceHandles = {
  destroy: () => void;
  applyRaw: (raw: string) => void;
  /** Promote Write-mode drafts into TipTap (for Cmd/Ctrl+S / plane switch). */
  flush: () => void;
};

export function mountSlidesSurface(
  host: HTMLElement,
  raw: string,
  liquidContext: LiquidRenderContext,
  onChange: (raw: string) => void,
): SlidesSurfaceHandles {
  let model = parseSlidesRaw(raw);
  let editing = false;
  let commitTimer: ReturnType<typeof setTimeout> | null = null;
  const root = document.createElement("div");
  root.className = `vault-live-slides ${embedWidthClass(model.width ?? "wide")}`;
  root.contentEditable = "false";

  const syncWidthClass = () => {
    root.className = `vault-live-slides ${embedWidthClass(model.width ?? "wide")}`;
  };

  const chrome = document.createElement("div");
  chrome.className = "vault-live-quiet-chrome vault-live-slides__chrome";

  const colGroup = document.createElement("div");
  colGroup.className = "vault-live-slides__cols";

  const syncColPressed = () => {
    for (const el of colGroup.querySelectorAll<HTMLButtonElement>(
      ".vault-live-slides__col",
    )) {
      const col = el.dataset.col ?? "";
      el.setAttribute("aria-pressed", col === model.columns ? "true" : "false");
    }
  };

  const stage = document.createElement("div");
  stage.className = "vault-live-slides__stage";
  let layoutWait: LayoutWaitHandle | null = null;

  const hydrateMount = (mount: HTMLElement) => {
    mountLiquidFence(mount, serializeSlidesRaw(model), liquidContext);
  };

  const showOrganism = () => {
    layoutWait?.cancel();
    layoutWait = null;
    editing = false;
    editBtn.textContent = "Write";
    unmountLiquidFence(stage);
    stage.replaceChildren();
    const mount = document.createElement("div");
    mount.className = "vault-live-slides__mount";
    stage.append(mount);
    layoutWait = whenElementHasLayout(stage, () => {
      layoutWait = null;
      if (!mount.isConnected) return;
      hydrateMount(mount);
    });
  };

  for (const col of ["1", "2", "3"] as const) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "vault-live-slides__col";
    btn.dataset.col = col;
    btn.textContent = `${col} col`;
    btn.setAttribute("aria-pressed", model.columns === col ? "true" : "false");
    btn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (editing) flushEditor();
      if (model.columns === col) return;
      model = { ...model, columns: col };
      syncColPressed();
      onChange(serializeSlidesRaw(model));
      if (!editing) showOrganism();
    });
    colGroup.append(btn);
  }

  const editBtn = document.createElement("button");
  editBtn.type = "button";
  editBtn.className = "vault-live-slides__edit";
  editBtn.textContent = "Write";
  editBtn.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  const widthBtn = document.createElement("button");
  widthBtn.type = "button";
  widthBtn.className = "vault-live-slides__width";
  const syncWidthLabel = () => {
    const w = model.width ?? "wide";
    widthBtn.textContent = w;
    widthBtn.title = `Width: ${w}`;
  };
  syncWidthLabel();
  widthBtn.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  widthBtn.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (editing) flushEditor();
    const cur = model.width ?? "wide";
    const idx = LIVE_EMBED_WIDTHS.indexOf(cur as SlidesDeckWidth);
    const next =
      LIVE_EMBED_WIDTHS[(idx + 1) % LIVE_EMBED_WIDTHS.length] ?? "wide";
    model = { ...model, width: next };
    syncWidthLabel();
    syncWidthClass();
    onChange(serializeSlidesRaw(model));
  });

  chrome.append(colGroup, widthBtn, editBtn);

  const commitFromEditor = (
    titleEl: HTMLInputElement,
    bodyEl: HTMLTextAreaElement,
  ) => {
    const text = bodyEl.value.replace(/\r\n/g, "\n");
    const next = parseSlidesDeck(text);
    if (next) {
      model = {
        ...next,
        title: titleEl.value.trim() || next.title,
        columns: next.columns || model.columns,
        width: next.width ?? model.width,
      };
    } else {
      model = { ...model, title: titleEl.value.trim() };
    }
    syncColPressed();
    onChange(serializeSlidesRaw(model));
  };

  const flushEditor = () => {
    if (commitTimer) {
      clearTimeout(commitTimer);
      commitTimer = null;
    }
    if (!editing) return;
    const title = stage.querySelector<HTMLInputElement>(".vault-live-slides__field");
    const body = stage.querySelector<HTMLTextAreaElement>(".vault-live-slides__body");
    if (title && body) commitFromEditor(title, body);
  };

  const showEditor = () => {
    layoutWait?.cancel();
    layoutWait = null;
    if (commitTimer) {
      clearTimeout(commitTimer);
      commitTimer = null;
    }
    editing = true;
    editBtn.textContent = "Done";
    unmountLiquidFence(stage);
    stage.replaceChildren();

    const title = document.createElement("input");
    title.className = "vault-live-slides__field";
    title.type = "text";
    title.placeholder = "Deck title";
    title.value = model.title;

    const body = document.createElement("textarea");
    body.className = "vault-live-slides__body";
    body.placeholder = "---\nlabel: Title\nlayout: hero\n\n# Slide…";
    body.value = serializeSlidesDeckBody(model).trim();
    body.rows = Math.min(22, Math.max(10, body.value.split("\n").length + 2));

    const scheduleCommit = () => {
      if (commitTimer) clearTimeout(commitTimer);
      commitTimer = setTimeout(() => {
        commitTimer = null;
        commitFromEditor(title, body);
      }, 160);
    };

    title.addEventListener("input", scheduleCommit);
    body.addEventListener("input", scheduleCommit);
    title.addEventListener("change", () => {
      if (commitTimer) clearTimeout(commitTimer);
      commitTimer = null;
      commitFromEditor(title, body);
    });
    body.addEventListener("change", () => {
      if (commitTimer) clearTimeout(commitTimer);
      commitTimer = null;
      commitFromEditor(title, body);
    });
    stage.append(title, body);
    title.focus();
  };

  editBtn.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (editing) {
      flushEditor();
      showOrganism();
      return;
    }
    showEditor();
  });

  const applyRaw = (nextRaw: string) => {
    model = parseSlidesRaw(nextRaw);
    syncColPressed();
    if (!editing) showOrganism();
  };

  root.append(chrome, stage);
  host.replaceChildren(root);
  showOrganism();

  const unregisterFlush = registerLiveDraftFlush(flushEditor);

  return {
    applyRaw,
    flush: flushEditor,
    destroy: () => {
      unregisterFlush();
      if (commitTimer) {
        clearTimeout(commitTimer);
        commitTimer = null;
      }
      layoutWait?.cancel();
      layoutWait = null;
      unmountLiquidFence(stage);
      host.replaceChildren();
    },
  };
}
