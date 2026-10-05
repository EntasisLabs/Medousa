import { Extension, Node, mergeAttributes } from "@tiptap/core";

/** Source metadata is internal, and must never appear in exported Markdown/HTML. */
export const MarkdownSourceIdentity = Extension.create({
  name: "markdownSourceIdentity",
  addGlobalAttributes() {
    return [{ types: ["paragraph", "heading", "bulletList", "orderedList", "taskList", "blockquote", "horizontalRule", "fenceBlock", "embedBlock", "table", "image", "footnoteDefinition", "markdownSourceBlock"], attributes: { sourceBlockId: { default: null, rendered: false } } }];
  },
});

/** A first-class source block for constructs the rich schema cannot represent. */
export const MarkdownSourceBlock = Node.create({
  name: "markdownSourceBlock", group: "block", atom: true, selectable: true,
  addAttributes() { return { raw: { default: "" } }; },
  parseHTML() { return [{ tag: "pre[data-markdown-source-block]" }]; },
  renderHTML({ node, HTMLAttributes }) {
    return ["pre", mergeAttributes(HTMLAttributes, { "data-markdown-source-block": "" }), ["code", node.attrs.raw]];
  },
  addNodeView() {
    return ({ node, editor, getPos }) => {
      let raw = String(node.attrs.raw);
      const dom = document.createElement("div");
      dom.className = "markdown-source-block";
      dom.contentEditable = "false";
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = "Edit source block";
      button.disabled = !editor.isEditable;
      const code = document.createElement("pre");
      code.textContent = raw;
      dom.append(button, code);
      const edit = () => {
        if (!editor.isEditable) return;
        const field = document.createElement("textarea");
        field.value = raw;
        field.setAttribute("aria-label", "Markdown source block");
        field.oninput = () => {
          const pos = typeof getPos === "function" ? getPos() : undefined;
          if (typeof pos === "number") editor.view.dispatch(editor.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, raw: field.value }));
        };
        const finish = (commit: boolean) => {
          if (commit && field.value !== raw) {
            const pos = typeof getPos === "function" ? getPos() : undefined;
            if (typeof pos === "number") editor.view.dispatch(editor.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, raw: field.value }));
          }
          field.replaceWith(code);
          code.textContent = raw;
          button.hidden = false;
        };
        field.onblur = () => finish(true);
        field.onkeydown = (event) => {
          if (event.key === "Escape") { event.preventDefault(); field.onblur = null; finish(false); }
          if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) { event.preventDefault(); field.blur(); }
        };
        button.hidden = true;
        code.replaceWith(field);
        field.focus();
      };
      button.onclick = edit;
      return { dom, stopEvent: (event) => event.target instanceof Element && !!event.target.closest("button, textarea"), update(next) {
        if (next.type !== node.type) return false;
        node = next;
        raw = String(next.attrs.raw);
        code.textContent = raw;
        button.disabled = !editor.isEditable;
        return true;
      } };
    };
  },
});
