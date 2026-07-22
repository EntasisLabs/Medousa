/**
 * Paragraph / heading blockId attrs — Obsidian trailing ` ^id` round-trip.
 */

import Paragraph from "@tiptap/extension-paragraph";
import Heading from "@tiptap/extension-heading";
import { Extension, mergeAttributes } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import type { Node as PMNode } from "@tiptap/pm/model";
import {
  appendBlockIdMarkdown,
  peelBlockIdFromInlineContent,
} from "$lib/markdown/blockAnchors";

const EMPTY_PARAGRAPH_MARKDOWN = "&nbsp;";
const NBSP_CHAR = "\u00A0";

const blockIdChipKey = new PluginKey<DecorationSet>("liveBlockIdChip");

function blockIdAttrSpec() {
  return {
    blockId: {
      default: null as string | null,
      parseHTML: (element: HTMLElement) => element.getAttribute("data-block-id"),
      renderHTML: (attributes: { blockId?: string | null }) => {
        if (!attributes.blockId) return {};
        // Live contenteditable: only data-block-id. Never set id="^…" here —
        // caret ids are invalid-ish in the editing DOM and have frozen WebViews.
        return {
          "data-block-id": attributes.blockId,
        };
      },
    },
  };
}

export const LiveParagraph = Paragraph.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      ...blockIdAttrSpec(),
    };
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "p",
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes),
      0,
    ];
  },

  parseMarkdown: (token, helpers) => {
    const tokens = token.tokens || [];
    if (tokens.length === 1 && tokens[0].type === "image") {
      return helpers.parseChildren([tokens[0]]);
    }
    const content = helpers.parseInline(tokens);
    const hasExplicitEmptyParagraphMarker =
      tokens.length === 1 &&
      tokens[0].type === "text" &&
      (tokens[0].raw === EMPTY_PARAGRAPH_MARKDOWN ||
        tokens[0].text === EMPTY_PARAGRAPH_MARKDOWN ||
        tokens[0].raw === NBSP_CHAR ||
        tokens[0].text === NBSP_CHAR);
    if (
      hasExplicitEmptyParagraphMarker &&
      content.length === 1 &&
      content[0].type === "text" &&
      (content[0].text === EMPTY_PARAGRAPH_MARKDOWN ||
        content[0].text === NBSP_CHAR)
    ) {
      return helpers.createNode("paragraph", undefined, []);
    }
    const peeled = peelBlockIdFromInlineContent(content);
    return helpers.createNode(
      "paragraph",
      peeled.blockId ? { blockId: peeled.blockId } : undefined,
      peeled.content,
    );
  },

  renderMarkdown: (node, h, ctx) => {
    if (!node) return "";
    const content = Array.isArray(node.content) ? node.content : [];
    if (content.length === 0) {
      const previousContent = Array.isArray(ctx?.previousNode?.content)
        ? ctx.previousNode.content
        : [];
      const previousNodeIsEmptyParagraph =
        ctx?.previousNode?.type === "paragraph" && previousContent.length === 0;
      const empty = previousNodeIsEmptyParagraph ? EMPTY_PARAGRAPH_MARKDOWN : "";
      return appendBlockIdMarkdown(empty, node.attrs?.blockId as string | null);
    }
    const body = h.renderChildren(content);
    return appendBlockIdMarkdown(body, node.attrs?.blockId as string | null);
  },
});

export const LiveHeading = Heading.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      ...blockIdAttrSpec(),
    };
  },

  renderHTML({ node, HTMLAttributes }) {
    const hasLevel = this.options.levels.includes(node.attrs.level);
    const level = hasLevel ? node.attrs.level : this.options.levels[0];
    return [
      `h${level}`,
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes),
      0,
    ];
  },

  parseMarkdown: (token, helpers) => {
    const content = helpers.parseInline(token.tokens || []);
    const peeled = peelBlockIdFromInlineContent(content);
    return helpers.createNode(
      "heading",
      {
        level: token.depth || 1,
        ...(peeled.blockId ? { blockId: peeled.blockId } : {}),
      },
      peeled.content,
    );
  },

  renderMarkdown: (node, h) => {
    const level = node.attrs?.level ? parseInt(String(node.attrs.level), 10) : 1;
    const headingChars = "#".repeat(level);
    if (!node.content) return "";
    const body = h.renderChildren(node.content);
    return `${headingChars} ${appendBlockIdMarkdown(body, node.attrs?.blockId as string | null)}`;
  },
});

function countBlockIdNodes(doc: PMNode): number {
  let count = 0;
  doc.descendants((node) => {
    if (node.type.name !== "paragraph" && node.type.name !== "heading") return;
    if (node.attrs.blockId) count += 1;
  });
  return count;
}

function buildBlockIdDecorations(doc: PMNode): DecorationSet {
  const decos: ReturnType<typeof Decoration.widget>[] = [];
  doc.descendants((node, pos) => {
    if (node.type.name !== "paragraph" && node.type.name !== "heading") {
      return;
    }
    const blockId = node.attrs.blockId as string | null | undefined;
    if (!blockId) return;
    // End of the textblock (inside the node). side: -1 keeps the widget in-flow
    // at the end rather than after the block (which can drift between paragraphs).
    const at = pos + node.nodeSize - 1;
    if (at < 0 || at > doc.content.size) return;
    const id = blockId;
    decos.push(
      Decoration.widget(
        at,
        () => {
          const widget = document.createElement("span");
          widget.className = "vault-live-block-id-chip";
          widget.contentEditable = "false";
          widget.textContent = `^${id}`;
          widget.title = `Block id ^${id}`;
          return widget;
        },
        {
          side: -1,
          ignoreSelection: true,
          key: `live-block-id:${id}`,
        },
      ),
    );
  });
  return DecorationSet.create(doc, decos);
}

/**
 * Quiet `^id` chip at end of blocks that carry blockId (source stays in attrs).
 * Maps decorations across typing transactions so widgets are not recreated
 * every keystroke (recreate → layout shift → scroll-into-view jump).
 */
export const LiveBlockIdChips = Extension.create({
  name: "liveBlockIdChips",

  addProseMirrorPlugins() {
    return [
      new Plugin<DecorationSet>({
        key: blockIdChipKey,
        state: {
          init: (_config, state) => buildBlockIdDecorations(state.doc),
          apply(tr, old, _oldState, newState) {
            if (!tr.docChanged) return old;
            const mapped = old.map(tr.mapping, tr.doc);
            // Typing inside a block only moves widgets; rebuild when ids appear/disappear.
            if (mapped.find().length === countBlockIdNodes(newState.doc)) {
              return mapped;
            }
            return buildBlockIdDecorations(newState.doc);
          },
        },
        props: {
          decorations(state) {
            return blockIdChipKey.getState(state) ?? DecorationSet.empty;
          },
        },
      }),
    ];
  },
});
