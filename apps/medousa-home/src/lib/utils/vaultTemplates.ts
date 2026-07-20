import { getSpaceById } from "$lib/config/vaultSpaces";

import {
  wrapWithFrontmatter,
  type VaultNoteKind,
} from "$lib/utils/vaultFrontmatter";
import {
  DEFAULT_KANBAN_COLUMNS,
  serializeKanbanColumns,
  wrapWithKanbanFrontmatter,
} from "$lib/utils/markdownKanban";

export const SLASH_VIEW_TEMPLATE = `\`\`\`medousa-view
from: projects/data.md
table: first
where: status != done
sort: due
columns: name, status, due
\`\`\`

`;

export const SLASH_TABLE_TEMPLATE = `| name | status | due |
| ---- | ------ | --- |
|  |  |  |

`;

export const SLASH_TOC_TEMPLATE = "```medousa-toc\n```\n\n";

export const SLASH_BOARD_TEMPLATE = `${serializeKanbanColumns(DEFAULT_KANBAN_COLUMNS)}\n\n`;

export function slugifyTitle(title: string): string {
  const slug = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "note";
}

export function isoDateLocal(date = new Date()): string {
  return date.toISOString().slice(0, 10);
}

export function dailyNotePath(date = new Date()): string {
  return `journal/${isoDateLocal(date)}.md`;
}

export function inboxCapturePath(date = new Date()): string {
  const stamp = date.toISOString().replace(/[:.]/g, "-");
  return `inbox/capture-${stamp}.md`;
}

/** Monday of the ISO week containing `date` (local timezone). */
export function isoWeekStart(date = new Date()): string {
  const copy = new Date(date);
  const day = copy.getDay();
  const diff = day === 0 ? -6 : 1 - day;
  copy.setDate(copy.getDate() + diff);
  return isoDateLocal(copy);
}

export function weeklyReviewPath(date = new Date()): string {
  return `journal/weekly-review-${isoWeekStart(date)}.md`;
}

export function weeklyReviewTitle(date = new Date()): string {
  return `Weekly Review · ${isoWeekStart(date)}`;
}

export function weeklyReviewWikilink(date = new Date()): string {
  return `[[${weeklyReviewTitle(date)}]]`;
}

export type VaultTemplateId =
  | "daily"
  | "weekly"
  | "project"
  | "board"
  | "database"
  | "view"
  | "ledger"
  | "inbox"
  | "bug"
  | "resume"
  | "blank";

export interface VaultTemplateOption {
  id: VaultTemplateId;
  label: string;
}

export const VAULT_TEMPLATES_BY_SPACE: Record<string, VaultTemplateOption[]> = {
  journal: [
    { id: "daily", label: "Daily note" },
    { id: "weekly", label: "Weekly review" },
    { id: "blank", label: "Blank journal note" },
  ],
  projects: [
    { id: "project", label: "Project" },
    { id: "board", label: "Kanban board" },
    { id: "database", label: "Database table" },
    { id: "view", label: "Query view" },
    { id: "resume", label: "Resume" },
  ],
  finance: [{ id: "ledger", label: "Ledger" }],
  inbox: [{ id: "inbox", label: "Quick capture" }],
  bugs: [{ id: "bug", label: "Bug report" }],
  other: [
    { id: "resume", label: "Resume" },
    { id: "blank", label: "Blank note" },
  ],
};

export function templatesForSpace(spaceId: string): VaultTemplateOption[] {
  return (
    VAULT_TEMPLATES_BY_SPACE[spaceId] ?? [
      { id: "resume", label: "Resume" },
      { id: "blank", label: "Blank note" },
    ]
  );
}

export function defaultTemplateForSpace(spaceId: string): VaultTemplateId {
  const options = templatesForSpace(spaceId);
  return options[0]?.id ?? "blank";
}

export function resolveTemplateForSpace(
  spaceId: string,
  templateId?: VaultTemplateId,
): VaultTemplateId {
  const options = templatesForSpace(spaceId);
  if (templateId && options.some((option) => option.id === templateId)) {
    return templateId;
  }
  return defaultTemplateForSpace(spaceId);
}

function withKind(kind: VaultNoteKind, body: string): string {
  return wrapWithFrontmatter(kind, body);
}

export function dailyNoteTemplate(date = new Date()): string {
  const label = isoDateLocal(date);
  return withKind(
    "daily",
    `# Daily · ${label}

## Notes

## Links

`,
  );
}

export function weeklyReviewTemplate(date = new Date()): string {
  const label = weeklyReviewTitle(date);
  return withKind(
    "daily",
    `# ${label}

## Wins

## Blockers

## Next week

## Links

`,
  );
}

export function inboxCaptureTemplate(line: string): string {
  const trimmed = line.trim();
  return withKind(
    "inbox",
    `# Capture

${trimmed}

`,
  );
}

export function blankNoteTemplate(title: string): string {
  const trimmed = title.trim();
  return withKind(
    "note",
    `# ${trimmed}

`,
  );
}

export function projectNoteTemplate(title: string): string {
  const trimmed = title.trim() || "Project";
  return withKind(
    "project",
    `# ${trimmed}

## Goal

## Next steps

## Links

`,
  );
}

export function projectBoardTemplate(title: string): string {
  const trimmed = title.trim() || "Board";
  return wrapWithKanbanFrontmatter(
    `# ${trimmed}\n\n${serializeKanbanColumns(DEFAULT_KANBAN_COLUMNS)}`,
  );
}

export function projectDatabaseTemplate(title: string): string {
  const trimmed = title.trim() || "Database";
  return withKind(
    "project",
    `# ${trimmed}

| name | status | due |
| ---- | ------ | --- |
| Example row | doing | ${isoDateLocal()} |

`,
  );
}

export function projectViewTemplate(title: string): string {
  const trimmed = title.trim() || "Active tasks";
  return withKind(
    "note",
    `# ${trimmed}

\`\`\`medousa-view
from: projects/data.md
table: first
where: status != done
sort: due
columns: name, status, due
\`\`\`

`,
  );
}

export function financeLedgerTemplate(title: string): string {
  const trimmed = title.trim() || "Ledger";
  return withKind(
    "ledger",
    `# ${trimmed}

> Type entries in the table below, or use **Link spreadsheet** to preview your Excel budget here — read-only, your file stays on disk.

| Date | Payee | Amount | Category |
| ---- | ----- | ------ | -------- |
|      |       |        |          |

`,
  );
}

export function bugNoteTemplate(title: string): string {
  const trimmed = title.trim() || "Bug";
  return withKind(
    "bug",
    `# ${trimmed}

## Repro

## Expected

## Actual

`,
  );
}

export function resumeNoteTemplate(title: string): string {
  const trimmed = title.trim() || "Resume";
  return withKind(
    "resume",
    `# ${trimmed}

City, ST | [email@example.com](mailto:email@example.com) | (555) 555-5555

## Professional summary

One or two sentences on the role you want and the strengths you bring.

## Areas of expertise

| Strength | Strength | Strength |
| -------- | -------- | -------- |
| Skill one | Skill two | Skill three |
| Skill four | Skill five | Skill six |
| Skill seven | Skill eight | Skill nine |

## Professional experience

### Role title

Company | Month Year – Present

- **Theme:** Concrete win with scope and outcome.
- **Theme:** Another result in plain language.

## Education

- School — Program or credential

## Technical skills

- Tooling and platforms you use daily

`,
  );
}

export function templateForSpace(spaceId: string, title: string): string {
  return contentForTemplate(defaultTemplateForSpace(spaceId), title);
}

export function contentForTemplate(
  templateId: VaultTemplateId,
  title: string,
  date = new Date(),
  spaceId?: string,
): string {
  switch (templateId) {
    case "daily":
      return dailyNoteTemplate(date);
    case "weekly":
      return weeklyReviewTemplate(date);
    case "project":
      return projectNoteTemplate(title);
    case "board":
      return projectBoardTemplate(title);
    case "database":
      return projectDatabaseTemplate(title);
    case "view":
      return projectViewTemplate(title);
    case "ledger":
      return financeLedgerTemplate(title);
    case "bug":
      return bugNoteTemplate(title);
    case "inbox":
      return inboxCaptureTemplate(title.trim() || "Captured thought");
    case "resume":
      return resumeNoteTemplate(title);
    case "blank":
    default: {
      const trimmed = title.trim() || "Note";
      const kind: VaultNoteKind = spaceId === "journal" ? "daily" : "note";
      return withKind(
        kind,
        `# ${trimmed}

`,
      );
    }
  }
}

export function pathForTemplate(
  templateId: VaultTemplateId,
  spaceId: string,
  title: string,
  date = new Date(),
): string | undefined {
  switch (templateId) {
    case "daily":
      return dailyNotePath(date);
    case "weekly":
      return weeklyReviewPath(date);
    case "inbox":
      return inboxCapturePath(date);
    case "resume":
      return `resumes/${slugifyTitle(title)}.md`;
    default: {
      const spaceConfig = getSpaceById(spaceId);
      const prefix = spaceConfig?.prefix;
      if (!prefix) return undefined;
      return `${prefix}${slugifyTitle(title)}.md`.replace(/\/+/g, "/");
    }
  }
}
