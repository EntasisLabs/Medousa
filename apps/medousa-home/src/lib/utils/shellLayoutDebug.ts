/** DEV: mark every layout box in the workshop shell — toggle via localStorage or ⌃⇧L */

export const SHELL_LAYOUT_DEBUG_KEY = "medousa-shell-layout-debug";

const CONTAINER_TAGS = new Set([
  "DIV",
  "SECTION",
  "NAV",
  "FOOTER",
  "HEADER",
  "MAIN",
  "ASIDE",
  "ARTICLE",
  "FORM",
  "UL",
  "OL",
  "LI",
  "BUTTON",
  "A",
  "FOOTER",
  "DIALOG",
]);

export type ShellDebugInsets = {
  top: number;
  right: number;
  bottom: number;
  left: number;
};

export type ShellDebugBox = {
  key: string;
  label: string;
  tag: string;
  depth: number;
  rect: { x: number; y: number; w: number; h: number; bottom: number; right: number };
  margin: ShellDebugInsets;
  padding: ShellDebugInsets;
  border: ShellDebugInsets;
  dataAttrs: Record<string, string>;
  isEmbedHost: boolean;
  isChrome: boolean;
};

export function isShellLayoutDebugEnabled(): boolean {
  return (
    import.meta.env.DEV &&
    typeof localStorage !== "undefined" &&
    localStorage.getItem(SHELL_LAYOUT_DEBUG_KEY) === "1"
  );
}

export function setShellLayoutDebugEnabled(on: boolean): void {
  if (typeof localStorage === "undefined") return;
  if (on) localStorage.setItem(SHELL_LAYOUT_DEBUG_KEY, "1");
  else localStorage.removeItem(SHELL_LAYOUT_DEBUG_KEY);
}

function meaningfulClass(el: HTMLElement): string | null {
  for (const token of el.classList) {
    if (
      token.startsWith("workshop-") ||
      token.startsWith("browser-") ||
      token.startsWith("human-browser-") ||
      token.startsWith("btn") ||
      token === "workshop-main" ||
      token === "workshop-status"
    ) {
      return token;
    }
  }
  return el.classList[0] ?? null;
}

function boxLabel(el: HTMLElement): string {
  const explicit = el.getAttribute("data-debug-label");
  if (explicit) return explicit;

  for (const attr of el.attributes) {
    if (attr.name.startsWith("data-browser-") || attr.name.startsWith("data-shell-")) {
      return attr.name.replace(/^data-/, "");
    }
  }

  if (el.id) return `#${el.id}`;
  const cls = meaningfulClass(el);
  if (cls) return cls;
  return el.tagName.toLowerCase();
}

function readInsets(style: CSSStyleDeclaration, prefix: "margin" | "padding" | "borderWidth"): ShellDebugInsets {
  const top = prefix === "borderWidth" ? style.borderTopWidth : style[`${prefix}Top` as "marginTop"];
  const right = prefix === "borderWidth" ? style.borderRightWidth : style[`${prefix}Right` as "marginRight"];
  const bottom = prefix === "borderWidth" ? style.borderBottomWidth : style[`${prefix}Bottom` as "marginBottom"];
  const left = prefix === "borderWidth" ? style.borderLeftWidth : style[`${prefix}Left` as "marginLeft"];
  return {
    top: Math.round(parseFloat(top) || 0),
    right: Math.round(parseFloat(right) || 0),
    bottom: Math.round(parseFloat(bottom) || 0),
    left: Math.round(parseFloat(left) || 0),
  };
}

function collectDataAttrs(el: HTMLElement): Record<string, string> {
  const out: Record<string, string> = {};
  for (const attr of el.attributes) {
    if (attr.name.startsWith("data-")) out[attr.name] = attr.value;
  }
  return out;
}

function shouldMark(el: HTMLElement, rect: DOMRect): boolean {
  if (rect.width < 2 || rect.height < 2) return false;
  if (el.closest("[data-shell-layout-debug-root]")) return false;
  const tag = el.tagName;
  if (CONTAINER_TAGS.has(tag)) return true;
  if (el.hasAttribute("data-debug-label")) return true;
  for (const attr of el.attributes) {
    if (attr.name.startsWith("data-browser-") || attr.name.startsWith("data-shell-")) return true;
  }
  return false;
}

/** Walk shell DOM and return every marked container with layout metrics. */
export function scanShellLayout(root: HTMLElement | null): ShellDebugBox[] {
  if (!root) return [];

  const boxes: ShellDebugBox[] = [];
  let seq = 0;

  const walk = (el: Element, depth: number) => {
    if (!(el instanceof HTMLElement)) return;

    const rect = el.getBoundingClientRect();
    if (shouldMark(el, rect)) {
      const style = getComputedStyle(el);
      if (style.display !== "none" && style.visibility !== "hidden") {
        const label = boxLabel(el);
        boxes.push({
          key: `${seq++}-${label}-${Math.round(rect.top)}`,
          label,
          tag: el.tagName.toLowerCase(),
          depth,
          rect: {
            x: Math.round(rect.left),
            y: Math.round(rect.top),
            w: Math.round(rect.width),
            h: Math.round(rect.height),
            bottom: Math.round(rect.bottom),
            right: Math.round(rect.right),
          },
          margin: readInsets(style, "margin"),
          padding: readInsets(style, "padding"),
          border: readInsets(style, "borderWidth"),
          dataAttrs: collectDataAttrs(el),
          isEmbedHost: el.hasAttribute("data-browser-embed-host"),
          isChrome: el.classList.contains("human-browser-chrome"),
        });
      }
    }

    for (const child of el.children) walk(child, depth + 1);
  };

  walk(root, 0);
  return boxes.sort((a, b) => a.rect.y - b.rect.y || a.rect.x - b.rect.x);
}

/** Vertical slices: what occupies Y from viewport top down to embed host top. */
export function chromeStackReport(boxes: ShellDebugBox[]): {
  embedHostTop: number | null;
  chromeBottom: number | null;
  gap: number | null;
  rows: Array<{ label: string; y: number; h: number; bottom: number; pad: string; margin: string }>;
} {
  const host = boxes.find((b) => b.isEmbedHost);
  const chrome = boxes.find((b) => b.isChrome);
  const embedHostTop = host?.rect.y ?? null;
  const chromeBottom = chrome?.rect.bottom ?? null;
  const gap = embedHostTop !== null && chromeBottom !== null ? embedHostTop - chromeBottom : null;

  const rows = boxes
    .filter((b) => embedHostTop === null || b.rect.bottom <= embedHostTop + 1)
    .filter((b) => b.rect.h >= 4)
    .map((b) => ({
      label: b.label,
      y: b.rect.y,
      h: b.rect.h,
      bottom: b.rect.bottom,
      pad: insetSummary(b.padding),
      margin: insetSummary(b.margin),
    }));

  return { embedHostTop, chromeBottom, gap, rows };
}

function insetSummary(insets: ShellDebugInsets): string {
  const parts: string[] = [];
  if (insets.top) parts.push(`t${insets.top}`);
  if (insets.right) parts.push(`r${insets.right}`);
  if (insets.bottom) parts.push(`b${insets.bottom}`);
  if (insets.left) parts.push(`l${insets.left}`);
  return parts.length ? parts.join(" ") : "—";
}

const DEPTH_HUES = [0, 210, 130, 330, 45, 280, 170, 20];

export function depthColor(depth: number, alpha = 0.85): string {
  const hue = DEPTH_HUES[depth % DEPTH_HUES.length];
  return `hsla(${hue}, 90%, 58%, ${alpha})`;
}
