/**
 * Live footnote definition block — round-trips `[^id]: text` (+ indented continuations).
 */

import { mergeAttributes, Node } from "@tiptap/core";

export const LiveFootnoteDefinition = Node.create({
  name: "footnoteDefinition",
  group: "block",
  content: "inline*",
  defining: true,

  addAttributes() {
    return {
      id: {
        default: "",
        parseHTML: (el) => el.getAttribute("data-footnote-id") ?? "",
        renderHTML: (attrs) =>
          attrs.id ? { "data-footnote-id": String(attrs.id) } : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "div[data-footnote-definition]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        class: "vault-live-footnote-def",
        "data-footnote-definition": "",
        "data-footnote-id": String(node.attrs.id ?? ""),
      }),
      0,
    ];
  },

  markdownTokenizer: {
    name: "footnoteDefinition",
    level: "block",
    start: "[^",
    tokenize: (src, _tokens, lexer) => {
      const head = /^\[\^([^\]]+)\]:\s?(.*?)(?:\n|$)/.exec(src);
      if (!head) return undefined;
      const id = (head[1] ?? "").trim();
      if (!id) return undefined;

      let raw = head[0];
      const parts = [head[2] ?? ""];
      let rest = src.slice(head[0].length);

      while (rest.length > 0) {
        const cont = /^(?: {2,}|\t)(.*?)(?:\n|$)/.exec(rest);
        if (!cont) break;
        parts.push(cont[1] ?? "");
        raw += cont[0];
        rest = rest.slice(cont[0].length);
      }

      const text = parts.join("\n").replace(/\s+$/, "");
      return {
        type: "footnoteDefinition",
        raw,
        id,
        text,
        tokens: lexer.inlineTokens(text),
      };
    },
  },

  parseMarkdown: (token, helpers) => {
    const id = typeof token.id === "string" ? token.id : "";
    return helpers.createNode(
      "footnoteDefinition",
      { id },
      helpers.parseInline(token.tokens || []),
    );
  },

  renderMarkdown: (node, h) => {
    const id = String(node.attrs?.id ?? "").trim();
    if (!id) return h.renderChildren(node);
    const body = h.renderChildren(node);
    const lines = body.split("\n");
    if (lines.length <= 1) {
      return `[^${id}]: ${body}\n\n`;
    }
    const [first, ...rest] = lines;
    return `[^${id}]: ${first}\n${rest.map((l) => `  ${l}`).join("\n")}\n\n`;
  },
});
