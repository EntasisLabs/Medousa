export interface KanbanCard {
  text: string;
  checked: boolean;
}

export interface KanbanColumn {
  title: string;
  cards: KanbanCard[];
}

const DEFAULT_COLUMNS: KanbanColumn[] = [
  { title: "Backlog", cards: [] },
  { title: "Doing", cards: [] },
  { title: "Done", cards: [] },
];

/** Parse `##` columns and Markdown task items from a kanban fence body. */
export function parseKanbanColumnsFromBody(body: string): KanbanColumn[] {
  const columns: KanbanColumn[] = [];
  let current: KanbanColumn | null = null;

  for (const line of body.replace(/\r\n/g, "\n").split("\n")) {
    const heading = line.match(/^##\s+(.+?)\s*$/);
    if (heading) {
      if (current) columns.push(current);
      current = { title: heading[1]!.trim(), cards: [] };
      continue;
    }
    const item = line.match(/^\s*-\s+\[([ xX])\]\s+(.*)$/);
    if (item && current) {
      current.cards.push({
        checked: item[1]!.toLowerCase() === "x",
        text: item[2]!.trimEnd(),
      });
    }
  }
  if (current) columns.push(current);

  return (columns.length > 0 ? columns : DEFAULT_COLUMNS).map((column) => ({
    title: column.title,
    cards: column.cards.map((card) => ({ ...card })),
  }));
}
