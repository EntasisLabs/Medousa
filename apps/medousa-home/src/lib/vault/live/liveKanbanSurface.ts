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
  isKanbanPointerDragging,
  resolveDropInsertIndex,
  startKanbanPointerDrag,
  type KanbanDragSource,
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

function gripIconSvg(): string {
  return `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" aria-hidden="true"><circle cx="9" cy="6" r="1.2"/><circle cx="15" cy="6" r="1.2"/><circle cx="9" cy="12" r="1.2"/><circle cx="15" cy="12" r="1.2"/><circle cx="9" cy="18" r="1.2"/><circle cx="15" cy="18" r="1.2"/></svg>`;
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
  let dragging: KanbanDragSource | null = null;

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

  /** Toggle drop/drag classes in place — never remount mid-drag (kills pointer capture). */
  const paintChrome = () => {
    board.querySelectorAll(".is-drop-target").forEach((el) => {
      el.classList.remove("is-drop-target");
    });
    board.querySelectorAll(".is-drop-before, .is-drop-after").forEach((el) => {
      el.classList.remove("is-drop-before", "is-drop-after");
    });
    board.querySelectorAll(".is-dragging").forEach((el) => {
      el.classList.remove("is-dragging");
    });

    if (dragging) {
      board
        .querySelector(
          `[data-column-index="${dragging.columnIndex}"][data-card-index="${dragging.cardIndex}"]`,
        )
        ?.classList.add("is-dragging");
    }

    if (!highlight) return;
    if (highlight.cardIndex == null) {
      board
        .querySelector(`[data-kanban-drop-column="${highlight.columnIndex}"]`)
        ?.classList.add("is-drop-target");
      return;
    }
    const card = board.querySelector(
      `[data-column-index="${highlight.columnIndex}"][data-card-index="${highlight.cardIndex}"]`,
    );
    card?.classList.add(
      highlight.insertBefore === false ? "is-drop-after" : "is-drop-before",
    );
  };

  const setHighlight = (target: KanbanDropTarget | null) => {
    highlight = target;
    // Drag util clears body class then calls onHighlight(null) on cancel —
    // drop the local dragging chrome too.
    if (target == null && dragging && !isKanbanPointerDragging()) {
      dragging = null;
    }
    paintChrome();
  };

  const setDragging = (source: KanbanDragSource | null) => {
    dragging = source;
    paintChrome();
  };

  const commit = (next: KanbanColumn[]) => {
    columns = next;
    onChange(serializeKanbanFence(columns));
    render();
  };

  const startCardDrag = (
    columnIndex: number,
    cardIndex: number,
    event: PointerEvent,
  ) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    setDragging({ columnIndex, cardIndex });
    startKanbanPointerDrag(
      { columnIndex, cardIndex },
      (target) => setHighlight(target),
      (from, to) => {
        const insertAt = resolveDropInsertIndex(to);
        setDragging(null);
        setHighlight(null);
        commit(
          moveKanbanCard(
            columns,
            from.columnIndex,
            from.cardIndex,
            to.columnIndex,
            insertAt,
          ),
        );
      },
      event,
    );
  };

  const render = () => {
    board.replaceChildren();
    columns.forEach((column, columnIndex) => {
      const col = document.createElement("div");
      col.className = "vault-live-mini-kanban__column";
      col.dataset.kanbanDropColumn = String(columnIndex);

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

        const grip = document.createElement("button");
        grip.type = "button";
        grip.className = "vault-live-mini-kanban__grip";
        grip.title = "Drag card";
        grip.setAttribute("aria-label", "Drag card");
        grip.innerHTML = gripIconSvg();
        grip.addEventListener("pointerdown", (event) => {
          startCardDrag(columnIndex, cardIndex, event);
        });
        // Keep TipTap from treating grip as text selection.
        grip.addEventListener("mousedown", (event) => {
          event.preventDefault();
          event.stopPropagation();
        });

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

        item.append(grip, text);
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

    // Re-apply chrome after rebuild (e.g. post-commit).
    paintChrome();
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
