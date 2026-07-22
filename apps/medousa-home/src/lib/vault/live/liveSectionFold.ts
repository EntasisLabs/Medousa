import { Extension } from "@tiptap/core";
import { Plugin, PluginKey, type EditorState, type Transaction } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import { LIVE_ICON_CHEVRON_DOWN, LIVE_ICON_CHEVRON_UP } from "./liveIcons";

type FoldState = { folded: Set<number> };

const foldKey = new PluginKey<FoldState>("liveSectionFold");

function headingLevel(node: { type: { name: string }; attrs: Record<string, unknown> }): number | null {
  if (node.type.name !== "heading") return null;
  const level = Number(node.attrs.level ?? 1);
  return Number.isFinite(level) ? Math.min(6, Math.max(1, level)) : 1;
}

/** End position (exclusive) of the foldable section starting at a heading. */
export function sectionFoldEnd(doc: EditorState["doc"], headingPos: number): number | null {
  const node = doc.nodeAt(headingPos);
  if (!node) return null;
  const level = headingLevel(node);
  if (level == null) return null;
  let scan = headingPos + node.nodeSize;
  while (scan < doc.content.size) {
    const child = doc.nodeAt(scan);
    if (!child) break;
    const childLevel = headingLevel(child);
    if (childLevel != null && childLevel <= level) break;
    scan += child.nodeSize;
  }
  return scan;
}

/** End of list-item fold: hide nested blocks after the first child. */
export function listItemFoldEnd(doc: EditorState["doc"], itemPos: number): number | null {
  const node = doc.nodeAt(itemPos);
  if (!node || node.type.name !== "listItem") return null;
  if (node.childCount <= 1) return null;
  return itemPos + node.nodeSize;
}

function toggleFold(tr: Transaction, pos: number): Transaction {
  const cur = foldKey.getState(tr)?.folded ?? new Set<number>();
  const next = new Set(cur);
  if (next.has(pos)) next.delete(pos);
  else next.add(pos);
  return tr.setMeta(foldKey, { folded: next });
}

function buildDecorations(state: EditorState): DecorationSet {
  const folded = foldKey.getState(state)?.folded ?? new Set<number>();
  const decos: Decoration[] = [];

  state.doc.descendants((node, pos) => {
    const isHeading = node.type.name === "heading";
    const isListItem = node.type.name === "listItem";
    if (!isHeading && !isListItem) return true;
    if (isListItem && node.childCount <= 1) return true;

    const isFolded = folded.has(pos);
    decos.push(
      Decoration.widget(
        pos + 1,
        (view) => {
          const el = document.createElement("button");
          el.type = "button";
          el.className = "vault-live-fold-btn";
          el.setAttribute("aria-label", isFolded ? "Expand section" : "Collapse section");
          el.setAttribute("aria-expanded", isFolded ? "false" : "true");
          el.innerHTML = isFolded ? LIVE_ICON_CHEVRON_DOWN : LIVE_ICON_CHEVRON_UP;
          el.addEventListener("mousedown", (event) => {
            event.preventDefault();
            event.stopPropagation();
          });
          el.addEventListener("click", (event) => {
            event.preventDefault();
            event.stopPropagation();
            view.dispatch(toggleFold(view.state.tr, pos));
          });
          return el;
        },
        { side: -1, ignoreSelection: true },
      ),
    );

    if (!isFolded) return true;

    const end = isHeading
      ? sectionFoldEnd(state.doc, pos)
      : listItemFoldEnd(state.doc, pos);
    if (end == null) return true;

    const from = isHeading
      ? pos + node.nodeSize
      : pos + 1 + (node.child(0)?.nodeSize ?? 0);
    if (from >= end) return true;

    state.doc.nodesBetween(from, end, (child, childPos) => {
      if (child.isBlock) {
        decos.push(
          Decoration.node(childPos, childPos + child.nodeSize, {
            class: "vault-live-section-folded",
          }),
        );
      }
      return false;
    });
    return true;
  });

  return DecorationSet.create(state.doc, decos);
}

/**
 * Obsidian-style fold: chevrons on headings / list items; hide following
 * section or nested list content. Session-local (not serialized).
 */
export const LiveSectionFold = Extension.create({
  name: "liveSectionFold",

  addProseMirrorPlugins() {
    return [
      new Plugin<FoldState>({
        key: foldKey,
        state: {
          init: () => ({ folded: new Set<number>() }),
          apply(tr, value) {
            const meta = tr.getMeta(foldKey) as FoldState | undefined;
            if (meta?.folded) return { folded: meta.folded };
            if (!tr.docChanged) return value;
            const next = new Set<number>();
            for (const pos of value.folded) {
              const mapped = tr.mapping.map(pos, -1);
              const node = tr.doc.nodeAt(mapped);
              if (
                node &&
                (node.type.name === "heading" || node.type.name === "listItem")
              ) {
                next.add(mapped);
              }
            }
            return { folded: next };
          },
        },
        props: {
          decorations(state) {
            return buildDecorations(state);
          },
        },
      }),
    ];
  },
});

/** Test helper: compute fold range for a heading/list item at pos. */
export function foldRangeForTest(
  doc: EditorState["doc"],
  pos: number,
): { from: number; to: number } | null {
  const node = doc.nodeAt(pos);
  if (!node) return null;
  if (node.type.name === "heading") {
    const to = sectionFoldEnd(doc, pos);
    if (to == null) return null;
    return { from: pos + node.nodeSize, to };
  }
  if (node.type.name === "listItem") {
    const to = listItemFoldEnd(doc, pos);
    if (to == null) return null;
    return { from: pos + 1 + (node.child(0)?.nodeSize ?? 0), to };
  }
  return null;
}
