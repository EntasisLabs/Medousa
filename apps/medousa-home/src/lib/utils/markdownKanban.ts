/** Phase E — markdown-native kanban boards (`medousa-board` frontmatter + ## columns). */

import { normalizeKind, stripFrontmatter } from "$lib/utils/vaultFrontmatter";

export interface KanbanCard {
  text: string;
  checked: boolean;
}

export interface KanbanColumn {
  title: string;
  cards: KanbanCard[];
}

export interface KanbanBoardRegion {
  columns: KanbanColumn[];
  /** Inclusive line indices in the full markdown (0-based). */
  startLine: number;
  endLine: number;
}

const TASK_ITEM_RE = /^(\s*)-\s+\[([ xX])\]\s+(.*)$/;

export const DEFAULT_KANBAN_COLUMNS: KanbanColumn[] = [
  { title: "Backlog", cards: [] },
  { title: "Doing", cards: [] },
  { title: "Done", cards: [] },
];

export function readMedousaBoardKind(markdown: string): string | null {
  const { frontmatter } = stripFrontmatter(markdown);
  if (!frontmatter) return null;
  for (const line of frontmatter.split("\n")) {
    const match = line.match(/^medousa-board:\s*(.+)$/i);
    if (match) return match[1].trim();
  }
  return null;
}

export function noteHasKanbanBoard(markdown: string): boolean {
  if (readMedousaBoardKind(markdown)) return true;
  const { frontmatter } = stripFrontmatter(markdown);
  if (!frontmatter) return false;
  for (const line of frontmatter.split("\n")) {
    if (!line.trimStart().startsWith("kind:")) continue;
    const value = line.slice(line.indexOf(":") + 1);
    return normalizeKind(value) === "board";
  }
  return false;
}

function bodyStartLineIndex(lines: string[]): number {
  let start = 0;
  while (start < lines.length && lines[start].trim() === "") start += 1;
  if (lines[start]?.trim() !== "---") return start;
  for (let i = start + 1; i < lines.length; i += 1) {
    if (lines[i].trim() === "---") return i + 1;
  }
  return start;
}

function findBoardInsertLine(lines: string[], bodyStart: number): number {
  let index = bodyStart;
  while (index < lines.length && lines[index].trim() === "") index += 1;
  if (lines[index]?.trim().startsWith("# ")) {
    index += 1;
    while (index < lines.length && lines[index].trim() === "") index += 1;
  }
  return index;
}

function parseH2(line: string): string | null {
  const match = line.match(/^##\s+(.+?)\s*$/);
  return match ? match[1].trim() : null;
}

function parseTaskItem(line: string): KanbanCard | null {
  const match = line.match(TASK_ITEM_RE);
  if (!match) return null;
  return {
    checked: match[2].toLowerCase() === "x",
    text: match[3].trimEnd(),
  };
}

export function serializeKanbanColumns(columns: KanbanColumn[]): string {
  return columns
    .map((column) => {
      const lines = [`## ${column.title}`];
      for (const card of column.cards) {
        const mark = card.checked ? "x" : " ";
        lines.push(`- [${mark}] ${card.text}`);
      }
      return lines.join("\n");
    })
    .join("\n\n");
}

/** Parse the kanban region from a note with `medousa-board` (or kind: board). */
export function findKanbanBoard(markdown: string): KanbanBoardRegion | null {
  if (!noteHasKanbanBoard(markdown)) return null;

  const lines = markdown.split("\n");
  const bodyStart = bodyStartLineIndex(lines);
  const columns: KanbanColumn[] = [];
  let regionStart = -1;
  let regionEnd = bodyStart - 1;
  let current: KanbanColumn | null = null;

  for (let i = bodyStart; i < lines.length; i += 1) {
    const line = lines[i];
    const heading = parseH2(line);
    if (heading) {
      if (current) columns.push(current);
      current = { title: heading, cards: [] };
      if (regionStart === -1) regionStart = i;
      regionEnd = i;
      continue;
    }

    const item = parseTaskItem(line);
    if (item && current) {
      current.cards.push(item);
      regionEnd = i;
      continue;
    }

    if (current && line.trim() !== "") {
      break;
    }
  }

  if (current) columns.push(current);

  if (regionStart === -1) {
    const insertAt = findBoardInsertLine(lines, bodyStart);
    return {
      columns: DEFAULT_KANBAN_COLUMNS.map((column) => ({
        title: column.title,
        cards: [...column.cards],
      })),
      startLine: insertAt,
      endLine: insertAt - 1,
    };
  }

  return {
    columns,
    startLine: regionStart,
    endLine: regionEnd,
  };
}

export function replaceKanbanBoard(
  markdown: string,
  columns: KanbanColumn[],
): string | null {
  const region = findKanbanBoard(markdown);
  if (!region) return null;

  const lines = markdown.split("\n");
  const replacement = serializeKanbanColumns(columns).split("\n");

  if (region.endLine < region.startLine) {
    const before = lines.slice(0, region.startLine);
    const after = lines.slice(region.startLine);
    const merged = [...before, ...replacement, ...after];
    return merged.join("\n").replace(/\n{3,}/g, "\n\n");
  }

  const before = lines.slice(0, region.startLine);
  const after = lines.slice(region.endLine + 1);
  return [...before, ...replacement, ...after].join("\n");
}

export function kanbanColumnsFromContent(markdown: string): KanbanColumn[] {
  const region = findKanbanBoard(markdown);
  if (!region) {
    return DEFAULT_KANBAN_COLUMNS.map((column) => ({
      title: column.title,
      cards: [...column.cards],
    }));
  }
  return region.columns.map((column) => ({
    title: column.title,
    cards: column.cards.map((card) => ({ ...card })),
  }));
}

export function wrapWithKanbanFrontmatter(body: string): string {
  const trimmed = body.replace(/^\n+/, "");
  return `---\nkind: board\nmedousa-board: basic\n---\n\n${trimmed}`;
}
