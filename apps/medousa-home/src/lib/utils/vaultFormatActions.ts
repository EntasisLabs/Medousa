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
