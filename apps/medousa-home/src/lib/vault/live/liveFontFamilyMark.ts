/**
 * Live font-family mark — round-trips `{{font:serif|text}}`.
 */

import { Mark, mergeAttributes } from "@tiptap/core";
import {
  isMarkdownFontFamily,
  normalizeMarkdownFontFamily,
  resolveMarkdownFontFamilyCss,
  type MarkdownFontFamily,
} from "$lib/utils/vaultMarkdownFonts";

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    fontFamily: {
      setFontFamily: (font: MarkdownFontFamily) => ReturnType;
      unsetFontFamily: () => ReturnType;
    };
  }
}

const FONT_OPEN_RE = /^\{\{font:(sans|serif|mono)\|/i;

export const LiveFontFamily = Mark.create({
  name: "fontFamily",

  addAttributes() {
    return {
      font: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-md-font"),
        renderHTML: (attributes) => {
          if (!attributes.font) return {};
          const css = resolveMarkdownFontFamilyCss(String(attributes.font));
          return {
            "data-md-font": attributes.font,
            style: css ? `font-family: ${css}` : undefined,
          };
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: "span[data-md-font]",
        getAttrs: (el) => {
          if (!(el instanceof HTMLElement)) return false;
          const font = el.getAttribute("data-md-font");
          return font && isMarkdownFontFamily(font) ? { font } : false;
        },
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { class: "vault-live-font-family" }),
      0,
    ];
  },

  addCommands() {
    return {
      setFontFamily:
        (font: MarkdownFontFamily) =>
        ({ commands }) => {
          const normalized = normalizeMarkdownFontFamily(font);
          if (!normalized) return false;
          return commands.setMark(this.name, { font: normalized });
        },
      unsetFontFamily:
        () =>
        ({ commands }) =>
          commands.unsetMark(this.name),
    };
  },

  markdownTokenizer: {
    name: "fontFamily",
    level: "inline",
    start: "{{font:",
    tokenize: (src, _tokens, lexer) => {
      const open = FONT_OPEN_RE.exec(src);
      if (!open) return undefined;
      const font = (open[1] ?? "").toLowerCase();
      if (!isMarkdownFontFamily(font)) return undefined;
      const afterOpen = open[0].length;
      const close = src.indexOf("}}", afterOpen);
      if (close < afterOpen) return undefined;
      const inner = src.slice(afterOpen, close);
      if (inner.includes("\n")) return undefined;
      return {
        type: "fontFamily",
        raw: src.slice(0, close + 2),
        text: inner,
        font,
        tokens: lexer.inlineTokens(inner),
      };
    },
  },

  parseMarkdown: (token, helpers) => {
    const font = typeof token.font === "string" ? token.font : "";
    return helpers.applyMark(
      "fontFamily",
      helpers.parseInline(token.tokens || []),
      { font },
    );
  },

  renderMarkdown: (node, h) => {
    const font = String(node.attrs?.font ?? "");
    const text = h.renderChildren(node);
    if (!font || !isMarkdownFontFamily(font)) return text;
    return `{{font:${font}|${text}}}`;
  },
});
