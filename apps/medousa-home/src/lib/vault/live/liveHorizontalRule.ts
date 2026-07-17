/**
 * Live divider — renders as a calm rule; click unrenders to editable `---`.
 */

import { mergeAttributes, Node, nodeInputRule } from "@tiptap/core";
import { TextSelection } from "@tiptap/pm/state";

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    horizontalRule: {
      setHorizontalRule: () => ReturnType;
    };
  }
}

export const LiveHorizontalRule = Node.create({
  name: "horizontalRule",
  group: "block",
  atom: true,
  selectable: true,

  parseHTML() {
    return [{ tag: "hr" }];
  },

  renderHTML({ HTMLAttributes }) {
    return ["hr", mergeAttributes(HTMLAttributes)];
  },

  addCommands() {
    return {
      setHorizontalRule:
        () =>
        ({ commands }) =>
          commands.insertContent({ type: this.name }),
    };
  },

  addInputRules() {
    return [
      nodeInputRule({
        find: /^(?:---|—-|___\s|\*\*\*\s)$/,
        type: this.type,
      }),
    ];
  },

  addNodeView() {
    return ({ editor, getPos }) => {
      const dom = document.createElement("div");
      dom.className = "vault-live-hr";
      dom.contentEditable = "false";
      dom.setAttribute("data-live-hr", "");
      dom.title = "Click to edit divider";

      const line = document.createElement("hr");
      line.className = "vault-live-hr__line";
      dom.append(line);

      const unrender = (e: Event) => {
        e.preventDefault();
        e.stopPropagation();
        const pos = typeof getPos === "function" ? getPos() : null;
        if (typeof pos !== "number") return;
        const { schema } = editor;
        const paragraph = schema.nodes.paragraph?.create(
          null,
          schema.text("---"),
        );
        if (!paragraph) return;
        const tr = editor.state.tr.replaceWith(pos, pos + 1, paragraph);
        const caret = Math.min(pos + 1 + 3, tr.doc.content.size);
        tr.setSelection(TextSelection.create(tr.doc, caret));
        editor.view.dispatch(tr);
        editor.view.focus();
      };

      dom.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      dom.addEventListener("click", unrender);

      return { dom, ignoreMutation: () => true };
    };
  },
});
