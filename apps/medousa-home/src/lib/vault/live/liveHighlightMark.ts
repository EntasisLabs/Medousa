/**
 * Live highlight mark — round-trips `==text==` via TipTap markdown.
 */

import { Mark, mergeAttributes } from "@tiptap/core";

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    highlight: {
      toggleHighlight: () => ReturnType;
      setHighlight: () => ReturnType;
      unsetHighlight: () => ReturnType;
    };
  }
}

export const LiveHighlight = Mark.create({
  name: "highlight",

  parseHTML() {
    return [
      { tag: "mark" },
      { tag: "span[data-vault-highlight]" },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "mark",
      mergeAttributes(HTMLAttributes, {
        class: "vault-live-highlight",
        "data-vault-highlight": "",
      }),
      0,
    ];
  },

  addCommands() {
    return {
      toggleHighlight:
        () =>
        ({ commands }) =>
          commands.toggleMark(this.name),
      setHighlight:
        () =>
        ({ commands }) =>
          commands.setMark(this.name),
      unsetHighlight:
        () =>
        ({ commands }) =>
          commands.unsetMark(this.name),
    };
  },

  markdownTokenizer: {
    name: "highlight",
    level: "inline",
    start: "==",
    tokenize: (src, _tokens, lexer) => {
      if (!src.startsWith("==")) return undefined;
      const close = src.indexOf("==", 2);
      if (close < 2) return undefined;
      const inner = src.slice(2, close);
      if (!inner || inner.includes("\n")) return undefined;
      return {
        type: "highlight",
        raw: src.slice(0, close + 2),
        text: inner,
        tokens: lexer.inlineTokens(inner),
      };
    },
  },

  parseMarkdown: (token, helpers) => {
    return helpers.applyMark(
      "highlight",
      helpers.parseInline(token.tokens || []),
    );
  },

  renderMarkdown: (node, h) => {
    const text = h.renderChildren(node);
    return `==${text}==`;
  },
});
