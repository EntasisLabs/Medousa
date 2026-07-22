import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import * as PMView from "@tiptap/pm/view";

type HeadingMarksState = { focused: boolean };

const headingMarksKey = new PluginKey<HeadingMarksState>("liveHeadingMarks");

export type HeadingMarksOptions = {
  /** When true, never show `#` widgets (optional Live WYSIWYG). */
  hideSyntax?: () => boolean;
};

/**
 * Show muted `#` / `##` / `###` when the caret is inside a heading and the
 * editor is focused. Resting (unfocused) Live shows no syntax graffiti.
 */
export const HeadingMarks = Extension.create<HeadingMarksOptions>({
  name: "liveHeadingMarks",

  addOptions() {
    return {
      hideSyntax: () => false,
    };
  },

  addProseMirrorPlugins() {
    const hideSyntax = () => this.options.hideSyntax?.() ?? false;
    return [
      new Plugin<HeadingMarksState>({
        key: headingMarksKey,
        state: {
          init: () => ({ focused: false }),
          apply(tr, value) {
            const meta = tr.getMeta(headingMarksKey);
            if (typeof meta === "boolean") return { focused: meta };
            return value;
          },
        },
        props: {
          decorations(state) {
            if (hideSyntax()) return PMView.DecorationSet.empty;
            const pluginState = headingMarksKey.getState(state);
            if (!pluginState?.focused) return PMView.DecorationSet.empty;

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
        view(editorView) {
          const setFocused = (focused: boolean) => {
            const cur = headingMarksKey.getState(editorView.state)?.focused;
            if (cur === focused) return;
            editorView.dispatch(
              editorView.state.tr.setMeta(headingMarksKey, focused),
            );
          };
          const onFocusIn = () => setFocused(true);
          const onFocusOut = (event: FocusEvent) => {
            const next = event.relatedTarget as Node | null;
            if (next && editorView.dom.contains(next)) return;
            setFocused(false);
          };
          editorView.dom.addEventListener("focusin", onFocusIn);
          editorView.dom.addEventListener("focusout", onFocusOut);
          setFocused(editorView.hasFocus());
          return {
            destroy() {
              editorView.dom.removeEventListener("focusin", onFocusIn);
              editorView.dom.removeEventListener("focusout", onFocusOut);
            },
          };
        },
      }),
    ];
  },
});
