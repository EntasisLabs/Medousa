<script lang="ts">
  import { GripVertical } from "@lucide/svelte";
  import {
    kanbanColumnsFromContent,
    replaceKanbanBoard,
    type KanbanColumn,
  } from "$lib/utils/markdownKanban";

  interface Props {
    content: string;
    disabled?: boolean;
    onchange: (nextContent: string) => void;
    onWikilink?: (target: string) => void;
  }

  let { content, disabled = false, onchange, onWikilink }: Props = $props();

  let columns = $state<KanbanColumn[]>([]);
  let syncedContent = $state("");
  let dragFrom = $state<{ columnIndex: number; cardIndex: number } | null>(null);

  $effect(() => {
    if (content === syncedContent) return;
    columns = kanbanColumnsFromContent(content);
    syncedContent = content;
  });

  function emitColumns(nextColumns: KanbanColumn[]) {
    columns = nextColumns;
    const updated = replaceKanbanBoard(content, nextColumns);
    if (updated) {
      syncedContent = updated;
      onchange(updated);
    }
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

  function updateCardText(columnIndex: number, cardIndex: number, text: string) {
    const next = cloneColumns();
    next[columnIndex].cards[cardIndex].text = text;
    emitColumns(next);
  }

  function toggleCard(columnIndex: number, cardIndex: number) {
    const next = cloneColumns();
    next[columnIndex].cards[cardIndex].checked =
      !next[columnIndex].cards[cardIndex].checked;
    emitColumns(next);
  }

  function addCard(columnIndex: number) {
    const next = cloneColumns();
    next[columnIndex].cards.push({ text: "", checked: false });
    emitColumns(next);
  }

  function removeCard(columnIndex: number, cardIndex: number) {
    const next = cloneColumns();
    next[columnIndex].cards = next[columnIndex].cards.filter(
      (_, index) => index !== cardIndex,
    );
    emitColumns(next);
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
      emitColumns(next);
      return;
    }
    const [card] = next[fromColumn].cards.splice(fromCard, 1);
    if (!card) return;
    const insertAt = toCard ?? next[toColumn].cards.length;
    next[toColumn].cards.splice(insertAt, 0, card);
    emitColumns(next);
  }

  function handleDragStart(columnIndex: number, cardIndex: number) {
    dragFrom = { columnIndex, cardIndex };
  }

  function handleDragOver(event: DragEvent) {
    event.preventDefault();
  }

  function handleDropOnColumn(columnIndex: number) {
    if (!dragFrom) return;
    moveCard(dragFrom.columnIndex, dragFrom.cardIndex, columnIndex);
    dragFrom = null;
  }

  function handleDropOnCard(columnIndex: number, cardIndex: number) {
    if (!dragFrom) return;
    moveCard(dragFrom.columnIndex, dragFrom.cardIndex, columnIndex, cardIndex);
    dragFrom = null;
  }

  function cardTextParts(text: string): Array<{ kind: "text" | "wikilink"; value: string }> {
    const parts: Array<{ kind: "text" | "wikilink"; value: string }> = [];
    const re = /\[\[([^\]]+)\]\]/g;
    let lastIndex = 0;
    let match: RegExpExecArray | null;
    while ((match = re.exec(text)) !== null) {
      if (match.index > lastIndex) {
        parts.push({ kind: "text", value: text.slice(lastIndex, match.index) });
      }
      parts.push({ kind: "wikilink", value: match[1] });
      lastIndex = match.index + match[0].length;
    }
    if (lastIndex < text.length) {
      parts.push({ kind: "text", value: text.slice(lastIndex) });
    }
    return parts.length > 0 ? parts : [{ kind: "text", value: text }];
  }

  function handleWikilinkClick(event: MouseEvent, target: string) {
    event.preventDefault();
    event.stopPropagation();
    onWikilink?.(target);
  }
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <div class="flex shrink-0 items-center justify-between gap-2 border-b border-surface-500/40 px-4 py-2">
    <p class="text-xs text-surface-400">Board view · columns are `##` headings in markdown</p>
  </div>

  <div class="min-h-0 flex-1 overflow-x-auto overflow-y-hidden p-3">
    <div class="flex h-full min-h-[240px] gap-2">
      {#each columns as column, columnIndex (column.title + columnIndex)}
        <section
          class="flex min-h-0 w-[min(17rem,78vw)] shrink-0 flex-col rounded-md border border-surface-500/40 bg-surface-900/35"
          role="region"
          aria-label="{column.title} column"
          ondragover={handleDragOver}
          ondrop={() => handleDropOnColumn(columnIndex)}
        >
          <header class="border-b border-surface-500/40 px-2.5 py-2">
            <input
              class="input w-full border-0 bg-transparent px-1 py-0.5 text-xs font-medium text-surface-100"
              type="text"
              value={column.title}
              {disabled}
              aria-label="Column title"
              oninput={(event) =>
                updateColumnTitle(
                  columnIndex,
                  (event.currentTarget as HTMLInputElement).value,
                )}
            />
            <p class="mt-1 px-1 text-[10px] tabular-nums text-surface-500">
              {column.cards.length} card{column.cards.length === 1 ? "" : "s"}
            </p>
          </header>

          <div class="min-h-0 flex-1 space-y-1.5 overflow-y-auto p-1.5">
            {#each column.cards as card, cardIndex (columnIndex + "-" + cardIndex)}
              <article
                class="workshop-kanban-card group relative rounded-md border border-surface-500/35 bg-surface-950/50 p-2"
                draggable={!disabled}
                ondragstart={() => handleDragStart(columnIndex, cardIndex)}
                ondragover={handleDragOver}
                ondrop={(event) => {
                  event.stopPropagation();
                  handleDropOnCard(columnIndex, cardIndex);
                }}
              >
                <div class="flex items-start gap-2">
                  <button
                    type="button"
                    class="mt-0.5 shrink-0 text-surface-600"
                    aria-hidden="true"
                    tabindex={-1}
                    disabled={disabled}
                  >
                    <GripVertical size={14} strokeWidth={1.75} />
                  </button>
                  <input
                    type="checkbox"
                    class="checkbox mt-0.5 shrink-0"
                    checked={card.checked}
                    {disabled}
                    aria-label="Mark card done"
                    onchange={() => toggleCard(columnIndex, cardIndex)}
                  />
                  <div class="min-w-0 flex-1">
                    <input
                      class="input w-full border-0 bg-transparent px-0 py-0 text-sm text-surface-100"
                      type="text"
                      value={card.text}
                      placeholder="Card text or [[wikilink]]"
                      {disabled}
                      oninput={(event) =>
                        updateCardText(
                          columnIndex,
                          cardIndex,
                          (event.currentTarget as HTMLInputElement).value,
                        )}
                    />
                    {#if card.text.includes("[[") && onWikilink}
                      <p class="mt-1 flex flex-wrap gap-1 text-[11px]">
                        {#each cardTextParts(card.text) as part, partIndex (partIndex)}
                          {#if part.kind === "wikilink"}
                            <button
                              type="button"
                              class="rounded bg-primary-500/15 px-1.5 py-0.5 text-primary-200 hover:bg-primary-500/25"
                              onclick={(event) => handleWikilinkClick(event, part.value)}
                            >
                              [[{part.value}]]
                            </button>
                          {/if}
                        {/each}
                      </p>
                    {/if}
                  </div>
                  <button
                    type="button"
                    class="btn btn-sm variant-ghost-surface shrink-0 px-1 opacity-0 transition group-hover:opacity-100"
                    {disabled}
                    aria-label="Remove card"
                    onclick={() => removeCard(columnIndex, cardIndex)}
                  >
                    ×
                  </button>
                </div>
              </article>
            {:else}
              <p class="px-2 py-4 text-center text-xs text-surface-500">Drop cards here</p>
            {/each}
          </div>

          <footer class="border-t border-surface-500/30 p-1.5">
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface w-full"
              {disabled}
              onclick={() => addCard(columnIndex)}
            >
              Add card
            </button>
          </footer>
        </section>
      {/each}
    </div>
  </div>
</div>
