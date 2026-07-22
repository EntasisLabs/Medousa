/** Shared format action catalog for Build format bar + Live selection bubble. */

import {
  Bold,
  Code,
  Heading1,
  Heading2,
  Heading3,
  Highlighter,
  Italic,
  Link,
  List,
  ListOrdered,
  SquareCheck,
  Type,
} from "@lucide/svelte";
import type { MarkdownFormatAction } from "$lib/utils/vaultMarkdownEdit";

export type VaultFormatActionItem = {
  action: MarkdownFormatAction;
  title: string;
  Icon: typeof Bold;
};

export type VaultFormatActionGroup = {
  label: string;
  items: VaultFormatActionItem[];
};

/** Build format bar — flat groups (still shows H1–H3 as siblings). */
export const VAULT_FORMAT_ACTION_GROUPS: VaultFormatActionGroup[] = [
  {
    label: "Style",
    items: [
      { action: "bold", title: "Bold", Icon: Bold },
      { action: "italic", title: "Italic", Icon: Italic },
      { action: "code", title: "Inline code", Icon: Code },
      { action: "highlight", title: "Highlight", Icon: Highlighter },
    ],
  },
  {
    label: "Structure",
    items: [
      { action: "h1", title: "Title", Icon: Heading1 },
      { action: "h2", title: "Section", Icon: Heading2 },
      { action: "h3", title: "Subsection", Icon: Heading3 },
    ],
  },
  {
    label: "Lists",
    items: [
      { action: "bullet", title: "Bullet list", Icon: List },
      { action: "numbered", title: "Numbered list", Icon: ListOrdered },
      { action: "checkbox", title: "Checkbox", Icon: SquareCheck },
    ],
  },
  {
    label: "Insert",
    items: [{ action: "link", title: "Link", Icon: Link }],
  },
];

/** Live bubble page 1 — emphasis toggles. */
export const VAULT_LIVE_EMPHASIS_ACTIONS: VaultFormatActionItem[] =
  VAULT_FORMAT_ACTION_GROUPS[0]!.items;

/** Live bubble page 1 — list toggles. */
export const VAULT_LIVE_LIST_ACTIONS: VaultFormatActionItem[] =
  VAULT_FORMAT_ACTION_GROUPS[2]!.items;

export const VAULT_LIVE_LINK_ACTION: VaultFormatActionItem =
  VAULT_FORMAT_ACTION_GROUPS[3]!.items[0]!;

/** Mutually exclusive block style — one state trigger + menu. */
export type VaultBlockStyleOption = {
  action: MarkdownFormatAction;
  short: string;
  label: string;
  Icon: typeof Bold;
};

export const VAULT_LIVE_BLOCK_STYLE_OPTIONS: VaultBlockStyleOption[] = [
  { action: "paragraph", short: "P", label: "Paragraph", Icon: Type },
  { action: "h1", short: "H1", label: "Title", Icon: Heading1 },
  { action: "h2", short: "H2", label: "Section", Icon: Heading2 },
  { action: "h3", short: "H3", label: "Subsection", Icon: Heading3 },
];

export function liveBlockStyleFromActions(
  active: Iterable<MarkdownFormatAction>,
): VaultBlockStyleOption {
  const set = active instanceof Set ? active : new Set(active);
  for (const opt of VAULT_LIVE_BLOCK_STYLE_OPTIONS) {
    if (opt.action !== "paragraph" && set.has(opt.action)) return opt;
  }
  return VAULT_LIVE_BLOCK_STYLE_OPTIONS[0]!;
}
