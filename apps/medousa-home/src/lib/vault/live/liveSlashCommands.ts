import type { Editor } from "@tiptap/core";
import type { SlashBlockId } from "$lib/utils/vaultMarkdownEdit";
import {
  SLASH_BOARD_TEMPLATE,
  SLASH_TOC_TEMPLATE,
} from "$lib/utils/vaultTemplates";
import {
  LIQUID_ACCORDION_TEMPLATE,
  LIQUID_ACTIONS_TEMPLATE,
  LIQUID_BRIEF_TEMPLATE,
  LIQUID_CALLOUT_TEMPLATE,
  LIQUID_CARD_TEMPLATE,
  LIQUID_CAROUSEL_TEMPLATE,
  LIQUID_CHIPS_TEMPLATE,
  LIQUID_CITE_TEMPLATE,
  LIQUID_CODE_TEMPLATE,
  LIQUID_COMPARE_TEMPLATE,
  LIQUID_DASHBOARD_TEMPLATE,
  LIQUID_DECISION_TEMPLATE,
  LIQUID_KANBAN_TEMPLATE,
  LIQUID_MEDIA_TEMPLATE,
  LIQUID_PLAN_TEMPLATE,
  LIQUID_REPORT_TEMPLATE,
  LIQUID_SECTION_TEMPLATE,
  LIQUID_SHORTLIST_TEMPLATE,
  LIQUID_SLIDES_TEMPLATE,
  LIQUID_STEPS_TEMPLATE,
  LIQUID_TABS_TEMPLATE,
  LIQUID_TIMELINE_TEMPLATE,
  LIQUID_TREE_TEMPLATE,
} from "$lib/utils/liquidFenceTemplates";
import { LIQUID_CHART_ARRIVAL_TEMPLATE } from "./liveChartSurface";

const LIQUID_TEMPLATES: Partial<Record<SlashBlockId, string>> = {
  liquid_callout: LIQUID_CALLOUT_TEMPLATE,
  liquid_card: LIQUID_CARD_TEMPLATE,
  liquid_carousel: LIQUID_CAROUSEL_TEMPLATE,
  liquid_actions: LIQUID_ACTIONS_TEMPLATE,
  liquid_section: LIQUID_SECTION_TEMPLATE,
  liquid_chips: LIQUID_CHIPS_TEMPLATE,
  liquid_media: LIQUID_MEDIA_TEMPLATE,
  liquid_cite: LIQUID_CITE_TEMPLATE,
  liquid_compare: LIQUID_COMPARE_TEMPLATE,
  liquid_plan: LIQUID_PLAN_TEMPLATE,
  liquid_timeline: LIQUID_TIMELINE_TEMPLATE,
  liquid_shortlist: LIQUID_SHORTLIST_TEMPLATE,
  liquid_decision: LIQUID_DECISION_TEMPLATE,
  liquid_brief: LIQUID_BRIEF_TEMPLATE,
  liquid_chart: LIQUID_CHART_ARRIVAL_TEMPLATE,
  liquid_dashboard: LIQUID_DASHBOARD_TEMPLATE,
  liquid_report: LIQUID_REPORT_TEMPLATE,
  liquid_slides: LIQUID_SLIDES_TEMPLATE,
  liquid_tabs: LIQUID_TABS_TEMPLATE,
  liquid_steps: LIQUID_STEPS_TEMPLATE,
  liquid_accordion: LIQUID_ACCORDION_TEMPLATE,
  liquid_code: LIQUID_CODE_TEMPLATE,
  liquid_tree: LIQUID_TREE_TEMPLATE,
  liquid_kanban: LIQUID_KANBAN_TEMPLATE,
};

/** Text before cursor in the current textblock (for `/filter`). */
export function liveSlashPrefix(editor: Editor): string | null {
  const { $from } = editor.state.selection;
  const parent = $from.parent;
  if (!parent.isTextblock) return null;
  const text = parent.textBetween(0, $from.parentOffset, "\n", "\n");
  const match = text.match(/^\s*\/([\w-]*)$/);
  return match ? (match[1] ?? "") : null;
}

export function liveSlashOpen(editor: Editor): boolean {
  return liveSlashPrefix(editor) != null;
}

/** Delete `/token` at the start of the current textblock. */
export function clearLiveSlash(editor: Editor): boolean {
  const { $from } = editor.state.selection;
  const parent = $from.parent;
  if (!parent.isTextblock) return false;
  const text = parent.textBetween(0, $from.parentOffset, "\n", "\n");
  const match = text.match(/^(\s*)(\/[\w-]*)$/);
  if (!match) return false;
  const from = $from.start();
  const to = $from.pos;
  return editor.chain().focus(undefined, { scrollIntoView: false }).deleteRange({ from, to }).run();
}

function focusChain(editor: Editor) {
  return editor.chain().focus(undefined, { scrollIntoView: false });
}

export function applyLiveSlashBlock(editor: Editor, block: SlashBlockId): boolean {
  clearLiveSlash(editor);

  const liquid = LIQUID_TEMPLATES[block];
  if (liquid) {
    return focusChain(editor).insertFenceBlock(liquid.trimEnd() + "\n").run();
  }

  switch (block) {
    case "h1":
      return focusChain(editor).toggleHeading({ level: 1 }).run();
    case "h2":
      return focusChain(editor).toggleHeading({ level: 2 }).run();
    case "h3":
      return focusChain(editor).toggleHeading({ level: 3 }).run();
    case "bullet":
      return focusChain(editor).toggleBulletList().run();
    case "numbered":
      return focusChain(editor).toggleOrderedList().run();
    case "checkbox":
      return focusChain(editor).toggleTaskList().run();
    case "quote":
      return focusChain(editor).toggleBlockquote().run();
    case "divider":
      return focusChain(editor).setHorizontalRule().run();
    case "link":
      return focusChain(editor)
        .insertContent("label")
        .setTextSelection({
          from: editor.state.selection.from - 5,
          to: editor.state.selection.from,
        })
        .setLink({ href: "https://" })
        .run();
    case "table":
      return focusChain(editor)
        .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
        .run();
    case "board":
      return focusChain(editor).insertContent(SLASH_BOARD_TEMPLATE).run();
    case "toc":
      return focusChain(editor).insertFenceBlock(SLASH_TOC_TEMPLATE).run();
    case "callout":
      return focusChain(editor)
        .insertContent("> [!note] Title\n> Body\n")
        .run();
    case "wikilink":
    case "embed":
    case "view":
      // Handled by host (pickers / bridges).
      return true;
    default:
      return false;
  }
}
