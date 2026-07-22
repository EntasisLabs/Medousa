/**
 * Live font-size mark — round-trips `{{size:lg|text}}` / `{{size:18|text}}`.
 */

import { Mark, mergeAttributes } from "@tiptap/core";
import {
  isMarkdownFontSizeToken,
  normalizeMarkdownFontSize,
  resolveMarkdownFontSizeCss,
} from "$lib/utils/vaultMarkdownFonts";

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    fontSize: {
      setFontSize: (size: string) => ReturnType;
      unsetFontSize: () => ReturnType;
    };
  }
}

const SIZE_OPEN_RE = /^\{\{size:([a-zA-Z0-9.]+)\|/;

export const LiveFontSize = Mark.create({
  name: "fontSize",

  addAttributes() {
    return {
      size: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-md-size"),
        renderHTML: (attributes) => {
          if (!attributes.size) return {};
          const css = resolveMarkdownFontSizeCss(String(attributes.size));
          return {
            "data-md-size": attributes.size,
            style: css ? `font-size: ${css}` : undefined,
          };
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: "span[data-md-size]",
        getAttrs: (el) => {
          if (!(el instanceof HTMLElement)) return false;
          const size = el.getAttribute("data-md-size");
          return size && isMarkdownFontSizeToken(size) ? { size } : false;
        },
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { class: "vault-live-font-size" }),
      0,
    ];
  },

  addCommands() {
    return {
      setFontSize:
        (size: string) =>
        ({ commands }) => {
          const normalized = normalizeMarkdownFontSize(size);
          if (!normalized) return false;
          return commands.setMark(this.name, { size: normalized });
        },
      unsetFontSize:
        () =>
        ({ commands }) =>
          commands.unsetMark(this.name),
    };
  },

  markdownTokenizer: {
    name: "fontSize",
    level: "inline",
    start: "{{size:",
    tokenize: (src, _tokens, lexer) => {
      const open = SIZE_OPEN_RE.exec(src);
      if (!open) return undefined;
      const size = (open[1] ?? "").toLowerCase();
      if (!isMarkdownFontSizeToken(size)) return undefined;
      const afterOpen = open[0].length;
      const close = src.indexOf("}}", afterOpen);
      if (close < afterOpen) return undefined;
      const inner = src.slice(afterOpen, close);
      if (inner.includes("\n")) return undefined;
      return {
        type: "fontSize",
        raw: src.slice(0, close + 2),
        text: inner,
        size,
        tokens: lexer.inlineTokens(inner),
      };
    },
  },

  parseMarkdown: (token, helpers) => {
    const size = typeof token.size === "string" ? token.size : "";
    return helpers.applyMark(
      "fontSize",
      helpers.parseInline(token.tokens || []),
      { size },
    );
  },

  renderMarkdown: (node, h) => {
    const size = String(node.attrs?.size ?? "");
    const text = h.renderChildren(node);
    if (!size || !isMarkdownFontSizeToken(size)) return text;
    return `{{size:${size}|${text}}}`;
  },
});
