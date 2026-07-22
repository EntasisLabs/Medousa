/**
 * Live inline footnote — round-trips `^[text]` via TipTap markdown.
 */

import { Mark, mergeAttributes } from "@tiptap/core";

export const LiveInlineFootnote = Mark.create({
  name: "inlineFootnote",

  parseHTML() {
    return [
      { tag: "sup[data-inline-footnote]" },
      { tag: "span[data-inline-footnote]" },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "sup",
      mergeAttributes(HTMLAttributes, {
        class: "vault-live-inline-footnote",
        "data-inline-footnote": "",
      }),
      0,
    ];
  },

  markdownTokenizer: {
    name: "inlineFootnote",
    level: "inline",
    start: "^[",
    tokenize: (src, _tokens, lexer) => {
      if (!src.startsWith("^[")) return undefined;
      const close = src.indexOf("]", 2);
      if (close < 2) return undefined;
      const inner = src.slice(2, close);
      if (!inner || inner.includes("\n")) return undefined;
      return {
        type: "inlineFootnote",
        raw: src.slice(0, close + 1),
        text: inner,
        tokens: lexer.inlineTokens(inner),
      };
    },
  },

  parseMarkdown: (token, helpers) => {
    return helpers.applyMark(
      "inlineFootnote",
      helpers.parseInline(token.tokens || []),
    );
  },

  renderMarkdown: (node, h) => {
    const text = h.renderChildren(node);
    return `^[${text}]`;
  },
});
