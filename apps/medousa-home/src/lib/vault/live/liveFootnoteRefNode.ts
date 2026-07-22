/**
 * Live footnote reference — round-trips `[^id]` as an inline atom.
 */

import { mergeAttributes, Node } from "@tiptap/core";

export const LiveFootnoteRef = Node.create({
  name: "footnoteRef",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,

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
    return [{ tag: "sup[data-footnote-ref]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const id = String(node.attrs.id ?? "");
    return [
      "sup",
      mergeAttributes(HTMLAttributes, {
        class: "vault-live-footnote-ref",
        "data-footnote-ref": "",
        title: id ? `[^${id}]` : "Footnote",
      }),
      id || "?",
    ];
  },

  markdownTokenizer: {
    name: "footnoteRef",
    level: "inline",
    start: "[^",
    tokenize: (src) => {
      const m = /^\[\^([^\]]+)\](?!:)/.exec(src);
      if (!m) return undefined;
      const id = (m[1] ?? "").trim();
      if (!id) return undefined;
      return {
        type: "footnoteRef",
        raw: m[0],
        id,
      };
    },
  },

  parseMarkdown: (token, helpers) => {
    const id = typeof token.id === "string" ? token.id : "";
    return helpers.createNode("footnoteRef", { id });
  },

  renderMarkdown: (node) => {
    const id = String(node.attrs?.id ?? "").trim();
    return id ? `[^${id}]` : "";
  },
});
