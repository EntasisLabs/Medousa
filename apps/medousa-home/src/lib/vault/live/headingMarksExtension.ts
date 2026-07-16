import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import * as PMView from "@tiptap/pm/view";

const headingMarksKey = new PluginKey("liveHeadingMarks");

/**
 * Show muted `#` / `##` / `###` prefix when the caret is inside a heading.
 * Marks are decorations (not editable); promote/demote keys handle level changes.
 */
export const HeadingMarks = Extension.create({
  name: "liveHeadingMarks",

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: headingMarksKey,
        props: {
          decorations(state) {
            const { selection, doc } = state;
            const { $from } = selection;
            let depth = $from.depth;
            while (depth > 0) {
              const node = $from.node(depth);
              if (node.type.name === "heading") {
                const pos = $from.before(depth);
                const level = Number(node.attrs.level ?? 1);
                const marks = "#".repeat(Math.min(3, Math.max(1, level)));
                const widget = document.createElement("span");
                widget.className = "vault-live-heading-marks";
                widget.textContent = `${marks} `;
                widget.contentEditable = "false";
                widget.setAttribute("aria-hidden", "true");
                return PMView.DecorationSet.create(doc, [
                  PMView.Decoration.widget(pos + 1, widget, {
                    side: -1,
                    ignoreSelection: true,
                  }),
                ]);
              }
              depth -= 1;
            }
            return PMView.DecorationSet.empty;
          },
        },
      }),
    ];
  },
});
