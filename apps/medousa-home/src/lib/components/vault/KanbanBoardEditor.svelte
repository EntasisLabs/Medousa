<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import { GripVertical, Plus, X } from "@lucide/svelte";
  import {
    kanbanColumnsFromContent,
    replaceKanbanBoard,
    type KanbanColumn,
  } from "$lib/utils/markdownKanban";
  import {
    cancelKanbanPointerDrag,
    currentKanbanDragSource,
    resolveDropInsertIndex,
    startKanbanPointerDrag,
    type KanbanDropTarget,
  } from "$lib/utils/vaultKanbanDrag";
  import VaultKanbanCardFace from "./VaultKanbanCardFace.svelte";
  import VaultKanbanNotePeek from "./VaultKanbanNotePeek.svelte";
  import { vault } from "$lib/stores/vault.svelte";

  interface Props {
    content: string;
    disabled?: boolean;
    onchange: (nextContent: string) => void;
    onWikilink?: (target: string) => void;
  }

  let { content, disabled = false, onchange, onWikilink }: Props = $props();

  let columns = $state<KanbanColumn[]>([]);
  let syncedContent = $state("");
  let dropHighlight = $state<KanbanDropTarget | null>(null);
  let editingCard = $state<{ column: number; card: number } | null>(null);
  let focusRequest = $state<{ column: number; card: number } | null>(null);
  let surfaceFocusRequest = $state<{
    column: number;
    card: number;
  } | null>(null);
  let columnTitleFocus = $state<number | null>(null);
  let peek = $state<{
    target: string;
    anchor: { top: number; left: number; bottom: number; width: number };
  } | null>(null);
  let emitTimer: ReturnType<typeof setTimeout> | null = null;

  function columnsEqual(left: KanbanColumn[], right: KanbanColumn[]): boolean {
    if (left.length !== right.length) return false;
    return left.every((column, columnIndex) => {
      const other = right[columnIndex];
      if (column.title !== other.title) return false;
      if (column.cards.length !== other.cards.length) return false;
      return column.cards.every(
        (card, cardIndex) =>
          card.text === other.cards[cardIndex].text &&
          card.checked === other.cards[cardIndex].checked,
      );
    });
  }

  $effect(() => {
    if (currentKanbanDragSource()) return;
    if (content === syncedContent) return;
    const parsedFromProp = kanbanColumnsFromContent(content);
    const parsedFromSynced = syncedContent
      ? kanbanColumnsFromContent(syncedContent)
      : null;
    if (parsedFromSynced && columnsEqual(parsedFromSynced, columns)) {
      return;
    }
    columns = parsedFromProp;
    syncedContent = content;
  });

  $effect(() => {
    const request = focusRequest;
    if (!request) return;
    void tick().then(() => {
      const el = document.querySelector(
        `[data-kanban-card-input="${request.column}-${request.card}"]`,
      ) as HTMLTextAreaElement | null;
      el?.focus();
      el?.select();
      focusRequest = null;
    });
  });

  $effect(() => {
    const request = surfaceFocusRequest;
    if (!request) return;
    void tick().then(() => {
      const el = document.querySelector(
        `[data-kanban-card-surface="${request.column}-${request.card}"]`,
      ) as HTMLElement | null;
      el?.focus();
      surfaceFocusRequest = null;
    });
  });

  $effect(() => {
    const index = columnTitleFocus;
    if (index == null) return;
    void tick().then(() => {
      const el = document.querySelector(
        `[data-kanban-column-title="${index}"]`,
      ) as HTMLInputElement | null;
      el?.focus();
      el?.select();
      columnTitleFocus = null;
    });
  });

  onDestroy(() => {
    if (emitTimer) clearTimeout(emitTimer);
    vault.setCompositionHold(false);
    cancelKanbanPointerDrag(() => {
      dropHighlight = null;
    });
  });

  function flushColumns(nextColumns: KanbanColumn[]) {
    const base = syncedContent || content;
    const updated = replaceKanbanBoard(base, nextColumns);
    if (updated) {
      syncedContent = updated;
      vault.setCompositionHold(false);
      onchange(updated);
    } else {
      vault.setCompositionHold(false);
    }
  }

  function emitColumns(
    nextColumns: KanbanColumn[],
    options?: { immediate?: boolean },
  ) {
    columns = nextColumns;
    if (emitTimer) {
      clearTimeout(emitTimer);
      emitTimer = null;
    }
    if (options?.immediate) {
      flushColumns(nextColumns);
      return;
    }
    // Hold autosave until the debounce settles — avoids save-echo proposal flicker.
    vault.setCompositionHold(true);
    emitTimer = setTimeout(() => {
      emitTimer = null;
      flushColumns(columns);
    }, 220);
  }

  function cloneColumns(): KanbanColumn[] {
    return columns.map((column) => ({
      title: column.title,
      cards: column.cards.map((card) => ({ ...card })),
    }));
  }

  function updateColumnTitle(columnIndex: number, title: string) {
    const next = cloneColumns();
    next[columnIndex].title = title;
    emitColumns(next);
  }

  function addColumn() {
    if (disabled) return;
    const next = cloneColumns();
    next.push({ title: "New column", cards: [] });
    emitColumns(next, { immediate: true });
    columnTitleFocus = next.length - 1;
  }

  function removeColumn(columnIndex: number) {
    if (disabled) return;
    if (columns.length <= 1) return;
    const column = columns[columnIndex];
    if (!column) return;
    if (
      column.cards.length > 0 &&
      !window.confirm(
        `Remove “${column.title}” and its ${column.cards.length} card${column.cards.length === 1 ? "" : "s"}?`,
      )
    ) {
      return;
    }
    const next = cloneColumns().filter((_, index) => index !== columnIndex);
    if (editingCard?.column === columnIndex) {
      editingCard = null;
    } else if (editingCard && editingCard.column > columnIndex) {
      editingCard = {
        column: editingCard.column - 1,
        card: editingCard.card,
      };
    }
    emitColumns(next, { immediate: true });
  }

  function updateCardText(columnIndex: number, cardIndex: number, text: string) {
    const next = cloneColumns();
    next[columnIndex].cards[cardIndex].text = text;
    emitColumns(next);
  }

  function toggleCard(columnIndex: number, cardIndex: number) {
    const next = cloneColumns();
    next[columnIndex].cards[cardIndex].checked =
      !next[columnIndex].cards[cardIndex].checked;
    emitColumns(next, { immediate: true });
  }

  function addCard(columnIndex: number, afterIndex?: number) {
    const next = cloneColumns();
    const card = { text: "", checked: false };
    if (afterIndex == null) {
      next[columnIndex].cards.push(card);
      emitColumns(next, { immediate: true });
      focusRequest = {
        column: columnIndex,
        card: next[columnIndex].cards.length - 1,
      };
    } else {
      next[columnIndex].cards.splice(afterIndex + 1, 0, card);
      emitColumns(next, { immediate: true });
      focusRequest = { column: columnIndex, card: afterIndex + 1 };
    }
    editingCard = focusRequest;
  }

  function removeCard(columnIndex: number, cardIndex: number) {
    const next = cloneColumns();
    next[columnIndex].cards = next[columnIndex].cards.filter(
      (_, index) => index !== cardIndex,
    );
    if (
      editingCard?.column === columnIndex &&
      editingCard.card === cardIndex
    ) {
      editingCard = null;
    }
    emitColumns(next, { immediate: true });
  }

  function moveCard(
    fromColumn: number,
    fromCard: number,
    toColumn: number,
    toCard?: number,
  ) {
    if (fromColumn === toColumn && fromCard === toCard) return;
    const next = cloneColumns();
    if (fromColumn === toColumn) {
      const cards = next[fromColumn].cards;
      const [card] = cards.splice(fromCard, 1);
      if (!card) return;
      let insertAt = toCard ?? cards.length;
      if (insertAt > fromCard) insertAt -= 1;
      cards.splice(insertAt, 0, card);
      emitColumns(next, { immediate: true });
      return;
    }
    const [card] = next[fromColumn].cards.splice(fromCard, 1);
    if (!card) return;
    const insertAt = toCard ?? next[toColumn].cards.length;
    next[toColumn].cards.splice(insertAt, 0, card);
    emitColumns(next, { immediate: true });
  }

  function reorderCardInColumn(
    columnIndex: number,
    cardIndex: number,
    delta: -1 | 1,
  ) {
    const target = cardIndex + delta;
    const cards = columns[columnIndex]?.cards;
    if (!cards || target < 0 || target >= cards.length) return;
    const next = cloneColumns();
    const lane = next[columnIndex].cards;
    const [card] = lane.splice(cardIndex, 1);
    if (!card) return;
    lane.splice(target, 0, card);
    emitColumns(next, { immediate: true });
    surfaceFocusRequest = { column: columnIndex, card: target };
  }

  function moveCardAcrossColumns(
    columnIndex: number,
    cardIndex: number,
    delta: -1 | 1,
  ) {
    const toColumn = columnIndex + delta;
    if (toColumn < 0 || toColumn >= columns.length) return;
    const insertAt = Math.min(cardIndex, columns[toColumn].cards.length);
    moveCard(columnIndex, cardIndex, toColumn, insertAt);
    surfaceFocusRequest = { column: toColumn, card: insertAt };
  }

  function handleGripPointerDown(
    event: PointerEvent,
    columnIndex: number,
    cardIndex: number,
  ) {
    if (disabled) return;
    event.preventDefault();
    event.stopPropagation();
    editingCard = null;
    startKanbanPointerDrag(
      { columnIndex, cardIndex },
      (target) => {
        dropHighlight = target;
      },
      (from, to) => {
        moveCard(
          from.columnIndex,
          from.cardIndex,
          to.columnIndex,
          resolveDropInsertIndex(to),
        );
      },
      event,
    );
  }

  function columnDropActive(columnIndex: number): boolean {
    return (
      dropHighlight?.columnIndex === columnIndex &&
      dropHighlight.cardIndex === undefined
    );
  }

  function cardDropBefore(columnIndex: number, cardIndex: number): boolean {
    return (
      dropHighlight?.columnIndex === columnIndex &&
      dropHighlight.cardIndex === cardIndex &&
      dropHighlight.insertBefore !== false
    );
  }

  function cardDropAfter(columnIndex: number, cardIndex: number): boolean {
    return (
      dropHighlight?.columnIndex === columnIndex &&
      dropHighlight.cardIndex === cardIndex &&
      dropHighlight.insertBefore === false
    );
  }

  function cardDragging(columnIndex: number, cardIndex: number): boolean {
    const source = currentKanbanDragSource();
    return (
      source?.columnIndex === columnIndex && source.cardIndex === cardIndex
    );
  }

  function isEditing(columnIndex: number, cardIndex: number): boolean {
    return (
      editingCard?.column === columnIndex && editingCard.card === cardIndex
    );
  }

  function beginEdit(columnIndex: number, cardIndex: number) {
    if (disabled) return;
    editingCard = { column: columnIndex, card: cardIndex };
    focusRequest = { column: columnIndex, card: cardIndex };
  }

  function handleCardBlur(columnIndex: number, cardIndex: number) {
    if (
      editingCard?.column !== columnIndex ||
      editingCard.card !== cardIndex
    ) {
      return;
    }
    editingCard = null;
    const card = columns[columnIndex]?.cards[cardIndex];
    if (card && !card.text.trim()) {
      removeCard(columnIndex, cardIndex);
    }
  }

  function handleCardKeydown(
    event: KeyboardEvent,
    columnIndex: number,
    cardIndex: number,
  ) {
    if (event.key === "Escape") {
      event.preventDefault();
      (event.currentTarget as HTMLTextAreaElement).blur();
      return;
    }
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    const text = columns[columnIndex]?.cards[cardIndex]?.text ?? "";
    if (!text.trim()) return;
    addCard(columnIndex, cardIndex);
  }

  function handleCardSurfaceKeydown(
    event: KeyboardEvent,
    columnIndex: number,
    cardIndex: number,
  ) {
    if (disabled || isEditing(columnIndex, cardIndex)) return;
    if (event.key === "Enter" || event.key === " ") {
      // Don't steal Space/Enter from checkbox or text controls.
      const target = event.target as HTMLElement | null;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLButtonElement
      ) {
        return;
      }
      event.preventDefault();
      beginEdit(columnIndex, cardIndex);
      return;
    }
    if (!(event.altKey || event.metaKey)) return;
    if (event.key === "ArrowUp") {
      event.preventDefault();
      reorderCardInColumn(columnIndex, cardIndex, -1);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      reorderCardInColumn(columnIndex, cardIndex, 1);
      return;
    }
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      moveCardAcrossColumns(columnIndex, cardIndex, -1);
      return;
    }
    if (event.key === "ArrowRight") {
      event.preventDefault();
      moveCardAcrossColumns(columnIndex, cardIndex, 1);
    }
  }

  function handleColumnKeydown(event: KeyboardEvent) {
    if (disabled) return;
    if (!(event.altKey || event.metaKey)) return;
    if (
      event.key !== "ArrowUp" &&
      event.key !== "ArrowDown" &&
      event.key !== "ArrowLeft" &&
      event.key !== "ArrowRight"
    ) {
      return;
    }
    const surface = (event.target as HTMLElement | null)?.closest(
      "[data-kanban-card-surface]",
    );
    if (!(surface instanceof HTMLElement)) return;
    const columnIndex = Number(surface.dataset.columnIndex);
    const cardIndex = Number(surface.dataset.cardIndex);
    if (Number.isNaN(columnIndex) || Number.isNaN(cardIndex)) return;
    handleCardSurfaceKeydown(event, columnIndex, cardIndex);
  }

  function handleWikilinkClick(target: string) {
    onWikilink?.(target);
  }

  function handlePeek(target: string, rect: DOMRect) {
    peek = {
      target,
      anchor: {
        top: rect.top,
        left: rect.left,
        bottom: rect.bottom,
        width: rect.width,
      },
    };
  }

  function isQuietColumn(title: string): boolean {
    return /^(done|complete|completed|shipped|finished)$/i.test(title.trim());
  }
</script>

{#if peek}
  <VaultKanbanNotePeek
    target={peek.target}
    anchor={peek.anchor}
    onClose={() => {
      peek = null;
    }}
    onOpen={handleWikilinkClick}
  />
{/if}

<div class="vault-kanban-board flex min-h-0 flex-1 flex-col overflow-hidden">
  <div class="vault-kanban-board-scroll min-h-0 flex-1 overflow-x-auto overflow-y-hidden p-3">
    <div class="vault-kanban-board-row">
      {#each columns as column, columnIndex (column.title + columnIndex)}
        {@const quiet = isQuietColumn(column.title)}
        {@const empty = column.cards.length === 0}
        <section
          class="vault-kanban-column {columnDropActive(columnIndex)
            ? 'vault-kanban-column--drop-active'
            : ''} {quiet ? 'vault-kanban-column--quiet' : ''} {quiet && empty
            ? 'vault-kanban-column--collapsed'
            : ''}"
          aria-label="{column.title} column"
          data-kanban-drop-column={columnIndex}
        >
          <header class="vault-kanban-column-header">
            <div class="vault-kanban-column-header-row">
              <input
                class="vault-kanban-column-title"
                type="text"
                value={column.title}
                {disabled}
                aria-label="Column title"
                data-kanban-column-title={columnIndex}
                oninput={(event) =>
                  updateColumnTitle(
                    columnIndex,
                    (event.currentTarget as HTMLInputElement).value,
                  )}
              />
              {#if !disabled && columns.length > 1}
                <button
                  type="button"
                  class="vault-kanban-column-remove"
                  aria-label="Remove column"
                  title="Remove column"
                  onclick={() => removeColumn(columnIndex)}
                >
                  <X size={12} strokeWidth={2} />
                </button>
              {/if}
            </div>
            <p class="vault-kanban-column-count">
              {column.cards.length} card{column.cards.length === 1 ? "" : "s"}
            </p>
          </header>

          <div class="vault-kanban-column-body">
            <!-- Alt+arrow reordering delegates from nested card controls (checkbox, grip). -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <div
              class="vault-kanban-card-stack"
              role="application"
              tabindex="-1"
              aria-label="{column.title} cards"
              onkeydown={handleColumnKeydown}
            >
              {#each column.cards as card, cardIndex (columnIndex + "-" + cardIndex)}
                <div
                  class="vault-kanban-card workshop-kanban-card group relative {cardDragging(
                    columnIndex,
                    cardIndex,
                  )
                    ? 'vault-kanban-card--dragging'
                    : ''} {cardDropBefore(columnIndex, cardIndex)
                    ? 'vault-kanban-card--drop-before'
                    : ''} {cardDropAfter(columnIndex, cardIndex)
                    ? 'vault-kanban-card--drop-after'
                    : ''} {card.checked ? 'vault-kanban-card--done' : ''} {isEditing(
                    columnIndex,
                    cardIndex,
                  )
                    ? 'vault-kanban-card--editing'
                    : ''}"
                  role="listitem"
                  data-kanban-drop-card
                  data-kanban-card-surface="{columnIndex}-{cardIndex}"
                  data-column-index={columnIndex}
                  data-card-index={cardIndex}
                >
                  <div
                    class="vault-kanban-card-row"
                    role="button"
                    tabindex={disabled || isEditing(columnIndex, cardIndex)
                      ? -1
                      : 0}
                    aria-label={card.text.trim() || "Empty card"}
                    onkeydown={(event) =>
                      handleCardSurfaceKeydown(event, columnIndex, cardIndex)}
                  >
                    <button
                      type="button"
                      class="vault-kanban-grip"
                      aria-label="Drag card"
                      title="Drag · Alt+↑↓ reorder · Alt+←→ column"
                      {disabled}
                      onpointerdown={(event) =>
                        handleGripPointerDown(event, columnIndex, cardIndex)}
                    >
                      <GripVertical size={14} strokeWidth={1.75} />
                    </button>
                    <label class="vault-kanban-check-wrap">
                      <input
                        type="checkbox"
                        class="vault-kanban-check"
                        checked={card.checked}
                        {disabled}
                        aria-label="Mark card done"
                        onchange={() => toggleCard(columnIndex, cardIndex)}
                      />
                      <span class="vault-kanban-check-face" aria-hidden="true"></span>
                    </label>
                    <div class="min-w-0 flex-1">
                      {#if isEditing(columnIndex, cardIndex) && !disabled}
                        <textarea
                          class="vault-kanban-card-input"
                          value={card.text}
                          placeholder="Write the card…"
                          rows={Math.min(6, Math.max(1, card.text.split("\n").length))}
                          data-kanban-card-input="{columnIndex}-{cardIndex}"
                          oninput={(event) =>
                            updateCardText(
                              columnIndex,
                              cardIndex,
                              (event.currentTarget as HTMLTextAreaElement).value,
                            )}
                          onblur={() => handleCardBlur(columnIndex, cardIndex)}
                          onkeydown={(event) =>
                            handleCardKeydown(event, columnIndex, cardIndex)}
                        ></textarea>
                      {:else}
                        <VaultKanbanCardFace
                          text={card.text}
                          checked={card.checked}
                          {disabled}
                          onEdit={() => beginEdit(columnIndex, cardIndex)}
                          onWikilink={onWikilink ? handleWikilinkClick : undefined}
                          onPeek={handlePeek}
                        />
                      {/if}
                    </div>
                    {#if !disabled}
                      <button
                        type="button"
                        class="vault-kanban-card-remove"
                        aria-label="Remove card"
                        onclick={() => removeCard(columnIndex, cardIndex)}
                      >
                        <X size={12} strokeWidth={2} />
                      </button>
                    {/if}
                  </div>
                </div>
              {:else}
                <div class="vault-kanban-empty" aria-hidden="true">
                  <p>{quiet ? "Clear" : "Quiet lane"}</p>
                  <span>{quiet ? "Finished work lands here" : "Add a card to begin"}</span>
                </div>
              {/each}
            </div>

            <button
              type="button"
              class="vault-kanban-add"
              {disabled}
              onclick={() => addCard(columnIndex)}
            >
              <Plus size={13} strokeWidth={2.25} />
              Add card
            </button>
          </div>
        </section>
      {/each}

      {#if !disabled}
        <button
          type="button"
          class="vault-kanban-add-column"
          aria-label="Add column"
          title="Add column"
          onclick={addColumn}
        >
          <Plus size={16} strokeWidth={2} />
          <span>Column</span>
        </button>
      {/if}
    </div>
  </div>
</div>
