/**
 * Liquid Live styled block — Type / Layout / ^ doors (progressive disclosure).
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

const ALIGN_GLYPH: Record<BlockAlign, string> = {
  left: "☰",
  center: "≡",
  right: "☰",
  justify: "≣",
};

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

function titleCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
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
    root.removeAttribute("id");
  } else {
    delete root.dataset.blockId;
    root.removeAttribute("id");
  }
}

type MenuId = "type" | "layout" | "id" | null;

export function mountBlockSurface(
  host: HTMLElement,
  raw: string,
  onChange: (raw: string) => void,
): BlockSurfaceHandles {
  let model: LiquidBlockProps =
    parseStyledBlockBody(fenceInner(raw)) ?? { body: " " };
  let openMenu: MenuId = null;

  const root = document.createElement("div");
  root.className = "liquid-styled-block vault-live-styled-block";
  root.contentEditable = "false";
  applyCss(root, model);

  const chrome = document.createElement("div");
  chrome.className = "vault-live-quiet-chrome vault-live-styled-block__chrome";

  const bar = document.createElement("div");
  bar.className = "vault-live-styled-block__bar";
  bar.setAttribute("role", "toolbar");
  bar.setAttribute("aria-label", "Block style");

  const typeBtn = document.createElement("button");
  typeBtn.type = "button";
  typeBtn.className = "vault-live-styled-block__door";
  typeBtn.setAttribute("aria-haspopup", "menu");

  const layoutBtn = document.createElement("button");
  layoutBtn.type = "button";
  layoutBtn.className = "vault-live-styled-block__door";
  layoutBtn.setAttribute("aria-haspopup", "menu");

  const idBtn = document.createElement("button");
  idBtn.type = "button";
  idBtn.className = "vault-live-styled-block__door vault-live-styled-block__door--id";
  idBtn.setAttribute("aria-haspopup", "dialog");

  const menuHost = document.createElement("div");
  menuHost.className = "vault-live-styled-block__menu-host";

  const stop = (e: Event) => {
    e.preventDefault();
    e.stopPropagation();
  };

  for (const btn of [typeBtn, layoutBtn, idBtn]) {
    btn.addEventListener("mousedown", stop);
  }

  const closeMenu = () => {
    openMenu = null;
    menuHost.replaceChildren();
    typeBtn.classList.remove("vault-live-styled-block__door--open");
    layoutBtn.classList.remove("vault-live-styled-block__door--open");
    idBtn.classList.remove("vault-live-styled-block__door--open");
    typeBtn.setAttribute("aria-expanded", "false");
    layoutBtn.setAttribute("aria-expanded", "false");
    idBtn.setAttribute("aria-expanded", "false");
  };

  const syncDoors = () => {
    const font = model.font ?? "sans";
    const size = (model.size ?? "md").toUpperCase();
    typeBtn.innerHTML = "";
    const typeLabel = document.createElement("span");
    typeLabel.className = `vault-live-styled-block__door-label vault-live-styled-block__door-label--${font}`;
    typeLabel.textContent = titleCase(font);
    const typeMeta = document.createElement("span");
    typeMeta.className = "vault-live-styled-block__door-meta";
    typeMeta.textContent = size;
    const typeChev = document.createElement("span");
    typeChev.className = "vault-live-styled-block__chev";
    typeChev.textContent = "▾";
    typeBtn.append(typeLabel, typeMeta, typeChev);
    typeBtn.title = `Type: ${titleCase(font)} · ${size}`;
    typeBtn.setAttribute("aria-label", typeBtn.title);

    const align = model.align ?? "left";
    layoutBtn.innerHTML = "";
    const alignGlyph = document.createElement("span");
    alignGlyph.className = "vault-live-styled-block__align-glyph";
    alignGlyph.dataset.align = align;
    alignGlyph.textContent = ALIGN_GLYPH[align];
    const layoutChev = document.createElement("span");
    layoutChev.className = "vault-live-styled-block__chev";
    layoutChev.textContent = "▾";
    layoutBtn.append(alignGlyph, layoutChev);
    layoutBtn.title = `Layout: ${align} · ${model.spacing ?? "normal"}`;
    layoutBtn.setAttribute("aria-label", layoutBtn.title);

    idBtn.textContent = "^";
    idBtn.title = model.id?.trim() ? `Block id ^${model.id.trim()}` : "Block id";
    idBtn.setAttribute("aria-label", idBtn.title);
    idBtn.classList.toggle(
      "vault-live-styled-block__door--has-id",
      Boolean(model.id?.trim()),
    );
  };

  const optionRow = (
    label: string,
    selected: boolean,
    onPick: () => void,
    opts?: { sampleClass?: string; meta?: string },
  ) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "vault-live-styled-block__option";
    if (selected) btn.classList.add("vault-live-styled-block__option--selected");
    btn.setAttribute("role", "menuitemradio");
    btn.setAttribute("aria-checked", selected ? "true" : "false");
    const main = document.createElement("span");
    main.className = "vault-live-styled-block__option-main";
    const lab = document.createElement("span");
    lab.className = "vault-live-styled-block__option-label";
    if (opts?.sampleClass) lab.classList.add(opts.sampleClass);
    lab.textContent = label;
    main.append(lab);
    if (opts?.meta) {
      const meta = document.createElement("span");
      meta.className = "vault-live-styled-block__option-meta";
      meta.textContent = opts.meta;
      main.append(meta);
    }
    btn.append(main);
    if (selected) {
      const check = document.createElement("span");
      check.className = "vault-live-styled-block__check";
      check.textContent = "✓";
      btn.append(check);
    }
    btn.addEventListener("mousedown", stop);
    btn.addEventListener("click", (e) => {
      stop(e);
      onPick();
    });
    return btn;
  };

  const sectionHeading = (text: string) => {
    const el = document.createElement("div");
    el.className = "vault-live-styled-block__menu-heading";
    el.textContent = text;
    return el;
  };

  const pickMeta = (patch: Partial<LiquidBlockProps>) => {
    const next = { ...model, ...patch };
    if (modelEqual(next, model)) return;
    model = next;
    applyCss(root, model);
    syncDoors();
    onChange(serializeStyledBlockFence(model));
  };

  const openTypeMenu = () => {
    const menu = document.createElement("div");
    menu.className = "vault-live-styled-block__menu";
    menu.setAttribute("role", "menu");
    menu.setAttribute("aria-label", "Type");
    menu.append(sectionHeading("Family"));
    for (const font of FONTS) {
      menu.append(
        optionRow(titleCase(font), (model.font ?? "sans") === font, () => {
          pickMeta({ font });
          renderMenu("type");
        }, { sampleClass: `vault-live-styled-block__sample--${font}` }),
      );
    }
    const sep = document.createElement("div");
    sep.className = "vault-live-styled-block__menu-sep";
    menu.append(sep, sectionHeading("Size"));
    const sizeRow = document.createElement("div");
    sizeRow.className = "vault-live-styled-block__size-row";
    const sizeLabel: Record<(typeof SIZES)[number], string> = {
      sm: "S",
      md: "M",
      lg: "L",
      xl: "XL",
    };
    for (const size of SIZES) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "vault-live-styled-block__size";
      if ((model.size ?? "md") === size) {
        btn.classList.add("vault-live-styled-block__size--selected");
      }
      btn.textContent = sizeLabel[size];
      btn.title = `Size: ${size}`;
      btn.addEventListener("mousedown", stop);
      btn.addEventListener("click", (e) => {
        stop(e);
        pickMeta({ size });
        renderMenu("type");
      });
      sizeRow.append(btn);
    }
    menu.append(sizeRow);
    menuHost.replaceChildren(menu);
  };

  const openLayoutMenu = () => {
    const menu = document.createElement("div");
    menu.className = "vault-live-styled-block__menu";
    menu.setAttribute("role", "menu");
    menu.setAttribute("aria-label", "Layout");
    menu.append(sectionHeading("Align"));
    for (const align of ALIGNS) {
      menu.append(
        optionRow(titleCase(align), (model.align ?? "left") === align, () => {
          pickMeta({ align });
          renderMenu("layout");
        }, { meta: ALIGN_GLYPH[align] }),
      );
    }
    const sep = document.createElement("div");
    sep.className = "vault-live-styled-block__menu-sep";
    menu.append(sep, sectionHeading("Spacing"));
    for (const spacing of SPACINGS) {
      menu.append(
        optionRow(titleCase(spacing), (model.spacing ?? "normal") === spacing, () => {
          pickMeta({ spacing });
          renderMenu("layout");
        }),
      );
    }
    menuHost.replaceChildren(menu);
  };

  const openIdMenu = () => {
    const menu = document.createElement("div");
    menu.className = "vault-live-styled-block__menu vault-live-styled-block__menu--id";
    menu.setAttribute("role", "dialog");
    menu.setAttribute("aria-label", "Block id");
    const row = document.createElement("div");
    row.className = "vault-live-styled-block__id-row";
    const caret = document.createElement("span");
    caret.className = "vault-live-styled-block__id-label";
    caret.textContent = "^";
    const input = document.createElement("input");
    input.type = "text";
    input.className = "vault-live-styled-block__id";
    input.placeholder = "id";
    input.value = model.id ?? "";
    input.addEventListener("mousedown", (e) => e.stopPropagation());
    const commitId = () => {
      pickMeta({ id: input.value.trim() || undefined });
    };
    input.addEventListener("blur", commitId);
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        input.blur();
        closeMenu();
      }
      if (e.key === "Escape") {
        e.preventDefault();
        closeMenu();
      }
    });
    row.append(caret, input);
    menu.append(row);
    menuHost.replaceChildren(menu);
    queueMicrotask(() => input.focus());
  };

  const renderMenu = (id: MenuId) => {
    if (id === "type") openTypeMenu();
    else if (id === "layout") openLayoutMenu();
    else if (id === "id") openIdMenu();
  };

  const toggleMenu = (id: MenuId) => {
    if (openMenu === id) {
      closeMenu();
      return;
    }
    openMenu = id;
    typeBtn.classList.toggle("vault-live-styled-block__door--open", id === "type");
    layoutBtn.classList.toggle("vault-live-styled-block__door--open", id === "layout");
    idBtn.classList.toggle("vault-live-styled-block__door--open", id === "id");
    typeBtn.setAttribute("aria-expanded", id === "type" ? "true" : "false");
    layoutBtn.setAttribute("aria-expanded", id === "layout" ? "true" : "false");
    idBtn.setAttribute("aria-expanded", id === "id" ? "true" : "false");
    renderMenu(id);
  };

  typeBtn.addEventListener("click", (e) => {
    stop(e);
    toggleMenu("type");
  });
  layoutBtn.addEventListener("click", (e) => {
    stop(e);
    toggleMenu("layout");
  });
  idBtn.addEventListener("click", (e) => {
    stop(e);
    toggleMenu("id");
  });

  const sep1 = document.createElement("span");
  sep1.className = "vault-live-styled-block__sep";
  sep1.setAttribute("aria-hidden", "true");
  const sep2 = document.createElement("span");
  sep2.className = "vault-live-styled-block__sep";
  sep2.setAttribute("aria-hidden", "true");

  bar.append(typeBtn, sep1, layoutBtn, sep2, idBtn);
  chrome.append(bar, menuHost);

  const body = document.createElement("div");
  body.className = "liquid-styled-block__body vault-live-styled-block__body";
  body.contentEditable = "true";
  body.spellcheck = true;
  body.textContent = model.body;
  body.dataset.placeholder = "Write…";
  body.addEventListener("blur", () => {
    const next: LiquidBlockProps = {
      ...model,
      body: normalizeBody(body.textContent),
    };
    if (modelEqual(next, model)) return;
    model = next;
    onChange(serializeStyledBlockFence(model));
  });

  const onDocPointer = (e: Event) => {
    if (!openMenu) return;
    const t = e.target;
    if (t instanceof Node && chrome.contains(t)) return;
    closeMenu();
  };
  document.addEventListener("mousedown", onDocPointer, true);

  syncDoors();
  root.append(chrome, body);
  host.replaceChildren(root);

  const setModel = (next: LiquidBlockProps) => {
    const prevBody = normalizeBody(body.textContent);
    model = { ...next };
    applyCss(root, model);
    syncDoors();
    if (openMenu) renderMenu(openMenu);
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

  return {
    root,
    setModel,
    applyRaw,
    destroy: () => {
      document.removeEventListener("mousedown", onDocPointer, true);
      closeMenu();
      host.replaceChildren();
    },
  };
}
