/**
 * Live text color mark — round-trips `{{red|text}}` / `{{#RRGGBB|text}}`.
 */

import { Mark, mergeAttributes } from "@tiptap/core";
import {
  isMarkdownColorToken,
  resolveMarkdownColorCss,
  type MarkdownColorToken,
} from "$lib/utils/vaultMarkdownColors";

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    textColor: {
      setTextColor: (color: MarkdownColorToken) => ReturnType;
      unsetTextColor: () => ReturnType;
    };
  }
}

const COLOR_OPEN_RE = /^\{\{([#a-zA-Z0-9]+)\|/;

export const LiveTextColor = Mark.create({
  name: "textColor",

  addAttributes() {
    return {
      color: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-md-color"),
        renderHTML: (attributes) => {
          if (!attributes.color) return {};
          const css = resolveMarkdownColorCss(String(attributes.color));
          return {
            "data-md-color": attributes.color,
            style: css ? `color: ${css}` : undefined,
          };
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: "span[data-md-color]",
        getAttrs: (el) => {
          if (!(el instanceof HTMLElement)) return false;
          const color = el.getAttribute("data-md-color");
          return color && isMarkdownColorToken(color) ? { color } : false;
        },
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { class: "vault-live-text-color" }),
      0,
    ];
  },

  addCommands() {
    return {
      setTextColor:
        (color: MarkdownColorToken) =>
        ({ commands }) => {
          if (!isMarkdownColorToken(String(color))) return false;
          return commands.setMark(this.name, { color: String(color) });
        },
      unsetTextColor:
        () =>
        ({ commands }) =>
          commands.unsetMark(this.name),
    };
  },

  markdownTokenizer: {
    name: "textColor",
    level: "inline",
    start: "{{",
    tokenize: (src, _tokens, lexer) => {
      const open = COLOR_OPEN_RE.exec(src);
      if (!open) return undefined;
      const color = open[1] ?? "";
      if (!isMarkdownColorToken(color)) return undefined;
      const afterOpen = open[0].length;
      const close = src.indexOf("}}", afterOpen);
      if (close < afterOpen) return undefined;
      const inner = src.slice(afterOpen, close);
      if (inner.includes("\n")) return undefined;
      return {
        type: "textColor",
        raw: src.slice(0, close + 2),
        text: inner,
        color,
        tokens: lexer.inlineTokens(inner),
      };
    },
  },

  parseMarkdown: (token, helpers) => {
    const color = typeof token.color === "string" ? token.color : "";
    return helpers.applyMark(
      "textColor",
      helpers.parseInline(token.tokens || []),
      { color },
    );
  },

  renderMarkdown: (node, h) => {
    const color = String(node.attrs?.color ?? "");
    const text = h.renderChildren(node);
    if (!color || !isMarkdownColorToken(color)) return text;
    return `{{${color}|${text}}}`;
  },
});
