/**
 * Liquid Live mini-kanban — in-note ```kanban fence with drag + card edit.
 */

import {
  moveKanbanCard,
  parseKanbanColumnsFromBody,
  serializeKanbanFence,
  type KanbanColumn,
} from "$lib/utils/markdownKanban";
import {
  resolveDropInsertIndex,
  startKanbanPointerDrag,
  type KanbanDropTarget,
} from "$lib/utils/vaultKanbanDrag";

function kanbanBody(raw: string): string {
  const open = /^```kanban[^\r\n]*\r?\n/i.exec(raw);
  const closeIdx = raw.lastIndexOf("\n```");
  if (open && closeIdx > open[0].length) {
    return raw.slice(open[0].length, closeIdx);
  }
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type KanbanSurfaceHandles = {
  destroy: () => void;
};

export function mountKanbanSurface(
  host: HTMLElement,
  raw: string,
  onChange: (raw: string) => void,
  onEditRaw?: () => void,
): KanbanSurfaceHandles {
  let columns: KanbanColumn[] = parseKanbanColumnsFromBody(kanbanBody(raw));
  let highlight: KanbanDropTarget | null = null;

  const root = document.createElement("div");
  root.className = "vault-live-mini-kanban";
  root.contentEditable = "false";

  const head = document.createElement("div");
  head.className = "vault-live-mini-kanban__head";

  const label = document.createElement("p");
  label.className = "vault-live-mini-kanban__label";
  label.textContent = "Board";

  const meta = document.createElement("div");
  meta.className = "vault-live-mini-kanban__meta vault-live-quiet-chrome";

  if (onEditRaw) {
    const source = document.createElement("button");
    source.type = "button";
    source.className = "vault-live-mini-kanban__source";
    source.textContent = "source";
    source.title = "Edit fence source";
    source.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    source.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      onEditRaw();
    });
    meta.append(source);
  }

  head.append(label, meta);

  const board = document.createElement("div");
  board.className = "vault-live-mini-kanban__board";

  const commit = (next: KanbanColumn[]) => {
    columns = next;
    onChange(serializeKanbanFence(columns));
    render();
  };

  const render = () => {
    board.replaceChildren();
    columns.forEach((column, columnIndex) => {
      const col = document.createElement("div");
      col.className = "vault-live-mini-kanban__column";
      col.dataset.kanbanDropColumn = String(columnIndex);
      if (highlight?.columnIndex === columnIndex && highlight.cardIndex == null) {
        col.classList.add("is-drop-target");
      }

      const title = document.createElement("p");
      title.className = "vault-live-mini-kanban__column-title";
      title.textContent = column.title;

      const list = document.createElement("div");
      list.className = "vault-live-mini-kanban__cards";

      column.cards.forEach((card, cardIndex) => {
        const item = document.createElement("div");
        item.className = "vault-live-mini-kanban__card";
        item.dataset.columnIndex = String(columnIndex);
        item.dataset.cardIndex = String(cardIndex);
        item.dataset.kanbanDropCard = "1";
        if (
          highlight?.columnIndex === columnIndex &&
          highlight.cardIndex === cardIndex
        ) {
          item.classList.add(
            highlight.insertBefore === false ? "is-drop-after" : "is-drop-before",
          );
        }

        const text = document.createElement("div");
        text.className = "vault-live-mini-kanban__card-text";
        text.contentEditable = "true";
        text.spellcheck = true;
        text.textContent = card.text;
        text.addEventListener("blur", () => {
          const nextText = text.textContent?.replace(/\u00a0/g, " ").trim() ?? "";
          if (nextText === card.text) return;
          const next = columns.map((c, i) =>
            i === columnIndex
              ? {
                  ...c,
                  cards: c.cards.map((entry, j) =>
                    j === cardIndex ? { ...entry, text: nextText || "Card" } : entry,
                  ),
                }
              : c,
          );
          commit(next);
        });
        text.addEventListener("keydown", (e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            text.blur();
          }
        });

        item.addEventListener("pointerdown", (event) => {
          if ((event.target as HTMLElement).closest("[contenteditable=true]")) return;
          if (event.button !== 0) return;
          event.preventDefault();
          startKanbanPointerDrag(
            { columnIndex, cardIndex },
            (target) => {
              highlight = target;
              render();
            },
            (from, to) => {
              const insertAt = resolveDropInsertIndex(to);
              commit(
                moveKanbanCard(
                  columns,
                  from.columnIndex,
                  from.cardIndex,
                  to.columnIndex,
                  insertAt,
                ),
              );
              highlight = null;
            },
            event,
          );
        });

        item.append(text);
        list.append(item);
      });

      const add = document.createElement("button");
      add.type = "button";
      add.className = "vault-live-mini-kanban__add";
      add.textContent = "+ Card";
      add.addEventListener("mousedown", (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      add.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        const next = columns.map((c, i) =>
          i === columnIndex
            ? { ...c, cards: [...c.cards, { text: "New card", checked: false }] }
            : c,
        );
        commit(next);
      });

      col.append(title, list, add);
      board.append(col);
    });
  };

  root.append(head, board);
  host.replaceChildren(root);
  render();

  return {
    destroy: () => {
      host.replaceChildren();
    },
  };
}
