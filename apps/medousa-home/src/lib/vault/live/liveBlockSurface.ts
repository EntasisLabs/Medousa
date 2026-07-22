/**
 * Liquid Live styled block — typography meta chrome + editable body.
 */

import {
  parseStyledBlockBody,
  serializeStyledBlockFence,
  styledBlockCssVars,
  type BlockAlign,
  type BlockFont,
  type LiquidBlockProps,
} from "$lib/markdown/styledBlock";

const FONTS: BlockFont[] = ["sans", "serif", "mono"];
const ALIGNS: BlockAlign[] = ["left", "center", "right", "justify"];
const SIZES = ["sm", "md", "lg", "xl"] as const;
const SPACINGS = ["tight", "normal", "relaxed"] as const;

function fenceInner(raw: string): string {
  const open = /^```block[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

function normalizeBody(text: string | null | undefined): string {
  return text?.replace(/\u00a0/g, " ").trim() || " ";
}

function modelEqual(a: LiquidBlockProps, b: LiquidBlockProps): boolean {
  return (
    a.body === b.body &&
    (a.id ?? "") === (b.id ?? "") &&
    (a.font ?? "") === (b.font ?? "") &&
    (a.size ?? "") === (b.size ?? "") &&
    (a.align ?? "") === (b.align ?? "") &&
    (a.spacing ?? "") === (b.spacing ?? "")
  );
}

export type BlockSurfaceHandles = {
  root: HTMLElement;
  setModel: (model: LiquidBlockProps) => void;
  /** In-place raw sync — avoids NodeView remount (layout jump / scroll fight). */
  applyRaw: (raw: string) => void;
  destroy: () => void;
};

function applyCss(root: HTMLElement, model: LiquidBlockProps): void {
  const vars = styledBlockCssVars(model);
  // Set/remove vars individually — never wipe cssText (that flashes layout).
  const keys = [
    "--block-font",
    "--block-size",
    "--block-align",
    "--block-spacing",
  ] as const;
  for (const key of keys) {
    const next = vars[key];
    if (next) root.style.setProperty(key, next);
    else root.style.removeProperty(key);
  }
  if (model.id?.trim()) {
    root.dataset.blockId = model.id.trim();
    // Prefer data-block-id in Live; caret ids belong in Preview HTML only.
    root.removeAttribute("id");
  } else {
    delete root.dataset.blockId;
    root.removeAttribute("id");
  }
}

export function mountBlockSurface(
  host: HTMLElement,
  raw: string,
  onChange: (raw: string) => void,
): BlockSurfaceHandles {
  let model: LiquidBlockProps =
    parseStyledBlockBody(fenceInner(raw)) ?? { body: " " };

  const root = document.createElement("div");
  root.className = "liquid-styled-block vault-live-styled-block";
  root.contentEditable = "false";
  applyCss(root, model);

  const chrome = document.createElement("div");
  chrome.className = "vault-live-quiet-chrome vault-live-styled-block__chrome";

  const addSeg = (
    label: string,
    values: readonly string[],
    current: string | undefined,
    onPick: (v: string) => void,
  ) => {
    const wrap = document.createElement("div");
    wrap.className = "vault-live-styled-block__seg";
    wrap.setAttribute("aria-label", label);
    for (const value of values) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "vault-live-styled-block__chip";
      btn.dataset.value = value;
      btn.textContent = value;
      btn.setAttribute("aria-pressed", value === current ? "true" : "false");
      btn.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      btn.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        onPick(value);
      });
      wrap.append(btn);
    }
    chrome.append(wrap);
  };

  const syncChips = () => {
    for (const wrap of chrome.querySelectorAll(".vault-live-styled-block__seg")) {
      const label = wrap.getAttribute("aria-label");
      let current = "";
      if (label === "Font") current = model.font ?? "sans";
      if (label === "Size") current = model.size ?? "md";
      if (label === "Align") current = model.align ?? "left";
      if (label === "Spacing") current = model.spacing ?? "normal";
      for (const btn of wrap.querySelectorAll<HTMLButtonElement>(".vault-live-styled-block__chip")) {
        btn.setAttribute(
          "aria-pressed",
          btn.dataset.value === current ? "true" : "false",
        );
      }
    }
    idInput.value = model.id ?? "";
  };

  const commit = () => {
    const next: LiquidBlockProps = {
      ...model,
      id: idInput.value.trim() || undefined,
      body: normalizeBody(body.textContent),
    };
    if (modelEqual(next, model)) return;
    model = next;
    applyCss(root, model);
    syncChips();
    onChange(serializeStyledBlockFence(model));
  };

  const pickMeta = (patch: Partial<LiquidBlockProps>) => {
    const next = { ...model, ...patch };
    if (modelEqual(next, model)) return;
    model = next;
    applyCss(root, model);
    syncChips();
    onChange(serializeStyledBlockFence(model));
  };

  addSeg("Font", FONTS, model.font ?? "sans", (v) => {
    pickMeta({ font: v as BlockFont });
  });
  addSeg("Size", SIZES, model.size ?? "md", (v) => {
    pickMeta({ size: v });
  });
  addSeg("Align", ALIGNS, model.align ?? "left", (v) => {
    pickMeta({ align: v as BlockAlign });
  });
  addSeg("Spacing", SPACINGS, model.spacing ?? "normal", (v) => {
    pickMeta({ spacing: v });
  });

  const idRow = document.createElement("div");
  idRow.className = "vault-live-styled-block__id-row";
  const idLabel = document.createElement("span");
  idLabel.className = "vault-live-styled-block__id-label";
  idLabel.textContent = "^";
  const idInput = document.createElement("input");
  idInput.type = "text";
  idInput.className = "vault-live-styled-block__id";
  idInput.placeholder = "block-id";
  idInput.value = model.id ?? "";
  idInput.addEventListener("mousedown", (e) => e.stopPropagation());
  idInput.addEventListener("blur", commit);
  idInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      idInput.blur();
    }
  });
  idRow.append(idLabel, idInput);
  chrome.append(idRow);

  const body = document.createElement("div");
  body.className = "liquid-styled-block__body vault-live-styled-block__body";
  body.contentEditable = "true";
  body.spellcheck = true;
  body.textContent = model.body;
  body.dataset.placeholder = "Write…";
  body.addEventListener("blur", commit);

  const setModel = (next: LiquidBlockProps) => {
    const prevBody = normalizeBody(body.textContent);
    model = { ...next };
    applyCss(root, model);
    syncChips();
    // Never rewrite body text if it already matches — that resets the caret
    // and makes the host scroll to keep the selection in view.
    if (normalizeBody(model.body) !== prevBody) {
      body.textContent = model.body;
    }
  };

  const applyRaw = (nextRaw: string) => {
    const parsed = parseStyledBlockBody(fenceInner(nextRaw));
    if (!parsed) return;
    if (modelEqual(parsed, model) && normalizeBody(body.textContent) === parsed.body) {
      return;
    }
    setModel(parsed);
  };

  root.append(chrome, body);
  host.replaceChildren(root);

  return {
    root,
    setModel,
    applyRaw,
    destroy: () => {
      host.replaceChildren();
    },
  };
}
