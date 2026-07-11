<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import { Check, Copy, Plus, X } from "@lucide/svelte";
  import {
    ledgerCsvFromContent,
    ledgerHeadersFromContent,
    ledgerProtectedColumnIndexes,
    ledgerRowsFromContent,
    replaceLedgerTable,
  } from "$lib/utils/markdownTable";
  import {
    mergeColumnMeta,
    parseLedgerColumns,
    resolveColumnAlign,
    resolveColumnType,
    serializeLedgerColumns,
    type LedgerColumn,
  } from "$lib/utils/ledgerSheet";
  import { MARKDOWN_COLOR_HEX } from "$lib/utils/vaultMarkdownColors";

  interface Props {
    content: string;
    disabled?: boolean;
    onchange: (nextContent: string) => void;
  }

  let { content, disabled = false, onchange }: Props = $props();

  let columns = $state<LedgerColumn[]>([]);
  let rows = $state<string[][]>([]);
  let syncedContent = $state("");
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  let selectionAnchor = $state<number | null>(null);
  let selectionFocus = $state<number | null>(null);
  let focusRequest = $state<{ row: number; col: number } | null>(null);
  let headerFocusRequest = $state<number | null>(null);
  let resizeDraftWidth = $state<string | null>(null);
  let resizingCol = $state<number | null>(null);

  const protectedColumns = $derived(
    ledgerProtectedColumnIndexes(columns.map((column) => column.label)),
  );

  const selectedRowIndexes = $derived.by(() => {
    if (selectionAnchor == null || selectionFocus == null) return new Set<number>();
    const start = Math.min(selectionAnchor, selectionFocus);
    const end = Math.max(selectionAnchor, selectionFocus);
    const next = new Set<number>();
    for (let i = start; i <= end; i += 1) next.add(i);
    return next;
  });

  $effect(() => {
    if (content === syncedContent) return;
    columns = parseLedgerColumns(ledgerHeadersFromContent(content));
    rows = ledgerRowsFromContent(content);
    syncedContent = content;
    selectionAnchor = null;
    selectionFocus = null;
    resizingCol = null;
    resizeDraftWidth = null;
  });

  $effect(() => {
    const request = focusRequest;
    if (!request) return;
    void tick().then(() => {
      const el = document.querySelector(
        `[data-ledger-cell="${request.row}-${request.col}"]`,
      ) as HTMLInputElement | null;
      el?.focus();
      el?.select();
      focusRequest = null;
    });
  });

  $effect(() => {
    const col = headerFocusRequest;
    if (col == null) return;
    void tick().then(() => {
      const el = document.querySelector(
        `[data-ledger-header="${col}"]`,
      ) as HTMLInputElement | null;
      el?.focus();
      el?.select();
      headerFocusRequest = null;
    });
  });

  onDestroy(() => {
    if (copyTimer) clearTimeout(copyTimer);
    detachResizeListeners();
  });

  function emitSheet(nextColumns: LedgerColumn[], nextRows: string[][]) {
    columns = nextColumns;
    rows = nextRows;
    const base = syncedContent || content;
    const updated = replaceLedgerTable(
      base,
      nextRows,
      serializeLedgerColumns(nextColumns),
    );
    if (updated) {
      syncedContent = updated;
      onchange(updated);
    }
  }

  function emptyRow(columnCount = columns.length): string[] {
    return Array.from({ length: columnCount }, () => "");
  }

  function updateCell(rowIndex: number, colIndex: number, value: string) {
    const next = rows.map((row, ri) =>
      ri === rowIndex
        ? row.map((cell, ci) => (ci === colIndex ? value : cell))
        : [...row],
    );
    emitSheet(columns, next);
  }

  function updateHeaderLabel(colIndex: number, value: string) {
    const nextColumns = columns.map((column, index) =>
      index === colIndex ? { ...column, label: value } : column,
    );
    emitSheet(nextColumns, rows);
  }

  function addRow(afterIndex?: number) {
    const next = rows.map((row) => [...row]);
    const row = emptyRow();
    if (afterIndex == null) {
      next.push(row);
      emitSheet(columns, next);
      focusRequest = { row: next.length - 1, col: 0 };
      return;
    }
    next.splice(afterIndex + 1, 0, row);
    emitSheet(columns, next);
    focusRequest = { row: afterIndex + 1, col: 0 };
  }

  function removeRows(indexes: number[]) {
    if (indexes.length === 0) return;
    const remove = new Set(indexes);
    let next = rows.filter((_, index) => !remove.has(index));
    if (next.length === 0) next = [emptyRow()];
    emitSheet(columns, next);
    selectionAnchor = null;
    selectionFocus = null;
  }

  function removeRow(rowIndex: number) {
    removeRows([rowIndex]);
  }

  function addColumn() {
    const label = `Column ${columns.length + 1}`;
    const nextColumns = [...columns, { label, meta: {} }];
    const nextRows = rows.map((row) => [...row, ""]);
    emitSheet(nextColumns, nextRows);
    headerFocusRequest = nextColumns.length - 1;
  }

  function removeColumn(colIndex: number) {
    if (protectedColumns.has(colIndex)) return;
    if (columns.length <= 2) return;
    const nextColumns = columns.filter((_, index) => index !== colIndex);
    const nextRows = rows.map((row) => row.filter((_, index) => index !== colIndex));
    emitSheet(nextColumns, nextRows);
  }

  function selectRow(rowIndex: number, event: MouseEvent) {
    if (disabled) return;
    if (event.shiftKey && selectionAnchor != null) {
      selectionFocus = rowIndex;
      return;
    }
    selectionAnchor = rowIndex;
    selectionFocus = rowIndex;
  }

  function clearSelection() {
    selectionAnchor = null;
    selectionFocus = null;
  }

  function columnWidth(column: LedgerColumn, colIndex: number): string | undefined {
    if (resizingCol === colIndex && resizeDraftWidth) return resizeDraftWidth;
    return column.meta.width;
  }

  function columnStyle(column: LedgerColumn, colIndex: number): string {
    const parts: string[] = [];
    const width = columnWidth(column, colIndex);
    if (width) {
      parts.push(`width:${width}`);
      parts.push(`min-width:${width}`);
      parts.push(`max-width:${width}`);
    }
    if (column.meta.color) {
      parts.push(`color:${MARKDOWN_COLOR_HEX[column.meta.color]}`);
    }
    return parts.join(";");
  }

  function isNumericColumn(column: LedgerColumn, colIndex: number): boolean {
    const type = resolveColumnType(column, colIndex);
    return type === "currency" || type === "number";
  }

  function isDateColumn(column: LedgerColumn, colIndex: number): boolean {
    return resolveColumnType(column, colIndex) === "date";
  }

  function alignClass(column: LedgerColumn, colIndex: number): string {
    const align = resolveColumnAlign(column, colIndex);
    if (align === "right") return "text-right";
    if (align === "center") return "text-center";
    return "text-left";
  }

  let resizeState: {
    colIndex: number;
    startX: number;
    startWidth: number;
  } | null = null;

  function detachResizeListeners() {
    window.removeEventListener("pointermove", handleResizeMove);
    window.removeEventListener("pointerup", handleResizeUp);
    window.removeEventListener("pointercancel", handleResizeUp);
  }

  function handleResizeMove(event: PointerEvent) {
    if (!resizeState) return;
    const delta = event.clientX - resizeState.startX;
    const next = Math.max(48, Math.round(resizeState.startWidth + delta));
    resizeDraftWidth = `${next}px`;
    resizingCol = resizeState.colIndex;
  }

  function handleResizeUp() {
    if (!resizeState) return;
    const colIndex = resizeState.colIndex;
    const width = resizeDraftWidth;
    resizeState = null;
    detachResizeListeners();
    resizingCol = null;
    resizeDraftWidth = null;
    if (!width) return;
    const nextColumns = columns.map((column, index) =>
      index === colIndex ? mergeColumnMeta(column, { width }) : column,
    );
    emitSheet(nextColumns, rows);
  }

  function startResize(event: PointerEvent, colIndex: number) {
    if (disabled) return;
    event.preventDefault();
    event.stopPropagation();
    const th = (event.currentTarget as HTMLElement).closest("th");
    const startWidth = th?.getBoundingClientRect().width ?? 120;
    resizeState = {
      colIndex,
      startX: event.clientX,
      startWidth,
    };
    resizingCol = colIndex;
    resizeDraftWidth = `${Math.round(startWidth)}px`;
    window.addEventListener("pointermove", handleResizeMove);
    window.addEventListener("pointerup", handleResizeUp);
    window.addEventListener("pointercancel", handleResizeUp);
  }

  function focusCell(row: number, col: number) {
    const safeRow = Math.max(0, Math.min(rows.length - 1, row));
    const safeCol = Math.max(0, Math.min(columns.length - 1, col));
    focusRequest = { row: safeRow, col: safeCol };
  }

  function handleCellKeydown(
    event: KeyboardEvent,
    rowIndex: number,
    colIndex: number,
  ) {
    if (disabled) return;

    if (event.key === "Escape") {
      event.preventDefault();
      (event.currentTarget as HTMLInputElement).blur();
      clearSelection();
      return;
    }

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (rowIndex >= rows.length - 1) {
        addRow();
      } else {
        focusCell(rowIndex + 1, colIndex);
      }
      return;
    }

    if (event.key === "Tab") {
      event.preventDefault();
      if (event.shiftKey) {
        if (colIndex > 0) focusCell(rowIndex, colIndex - 1);
        else if (rowIndex > 0) focusCell(rowIndex - 1, columns.length - 1);
      } else if (colIndex < columns.length - 1) {
        focusCell(rowIndex, colIndex + 1);
      } else if (rowIndex < rows.length - 1) {
        focusCell(rowIndex + 1, 0);
      } else {
        addRow();
      }
      return;
    }

    const input = event.currentTarget as HTMLInputElement;
    const atStart = input.selectionStart === 0 && input.selectionEnd === 0;
    const atEnd =
      input.selectionStart === input.value.length &&
      input.selectionEnd === input.value.length;

    if (event.key === "ArrowLeft" && atStart && colIndex > 0) {
      event.preventDefault();
      focusCell(rowIndex, colIndex - 1);
      return;
    }
    if (event.key === "ArrowRight" && atEnd && colIndex < columns.length - 1) {
      event.preventDefault();
      focusCell(rowIndex, colIndex + 1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (rowIndex > 0) focusCell(rowIndex - 1, colIndex);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (rowIndex < rows.length - 1) focusCell(rowIndex + 1, colIndex);
      else addRow();
    }
  }

  function handleSheetKeydown(event: KeyboardEvent) {
    if (disabled) return;
    if (selectedRowIndexes.size === 0) return;
    const target = event.target as HTMLElement | null;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement
    ) {
      return;
    }
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      removeRows([...selectedRowIndexes]);
    }
  }

  async function copyCsv() {
    const csv = ledgerCsvFromContent(syncedContent || content);
    if (!csv) return;
    try {
      await navigator.clipboard.writeText(csv);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied = false;
      }, 1600);
    } catch {
      // Clipboard may be unavailable in Tauri webview.
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="ledger-sheet flex min-h-0 flex-1 flex-col overflow-hidden"
  class:ledger-sheet--resizing={resizingCol != null}
  onkeydown={handleSheetKeydown}
>
  <div class="ledger-toolbar flex shrink-0 items-center justify-end gap-2 border-b border-surface-500/40 px-4 py-2">
    <button
      type="button"
      class="btn btn-sm variant-ghost-surface gap-1.5"
      {disabled}
      onclick={copyCsv}
    >
      {#if copied}
        <Check size={14} strokeWidth={2} />
        Copied
      {:else}
        <Copy size={14} strokeWidth={2} />
        Copy CSV
      {/if}
    </button>
  </div>

  <div class="min-h-0 flex-1 overflow-auto p-4">
    <table class="ledger-table w-full min-w-[560px] border-collapse text-sm">
      <colgroup>
        <col class="ledger-col-gutter" />
        {#each columns as column, colIndex (colIndex)}
          <col style={columnWidth(column, colIndex) ? `width:${columnWidth(column, colIndex)}` : undefined} />
        {/each}
        <col class="ledger-col-add" />
        <col class="ledger-col-actions" />
      </colgroup>
      <thead>
        <tr class="ledger-header-row border-b border-surface-500/50 text-left">
          <th class="ledger-gutter-head" scope="col">
            <span class="sr-only">Row</span>
            #
          </th>
          {#each columns as column, colIndex (colIndex)}
            <th
              class="ledger-header-cell {alignClass(column, colIndex)} {isDateColumn(
                column,
                colIndex,
              )
                ? 'ledger-header-date'
                : ''}"
              scope="col"
              style={columnStyle(column, colIndex)}
            >
              <div class="ledger-header-edit">
                <input
                  class="ledger-header-input {alignClass(column, colIndex)}"
                  type="text"
                  value={column.label}
                  {disabled}
                  aria-label="Column {colIndex + 1} name"
                  data-ledger-header={colIndex}
                  oninput={(event) =>
                    updateHeaderLabel(
                      colIndex,
                      (event.currentTarget as HTMLInputElement).value,
                    )}
                />
                {#if !disabled && !protectedColumns.has(colIndex)}
                  <button
                    type="button"
                    class="ledger-col-remove"
                    title="Remove column"
                    aria-label="Remove column {column.label}"
                    onclick={() => removeColumn(colIndex)}
                  >
                    <X size={11} strokeWidth={2} />
                  </button>
                {/if}
              </div>
              {#if !disabled}
                <button
                  type="button"
                  class="ledger-col-resize"
                  aria-label="Resize column {column.label}"
                  title="Drag to resize"
                  onpointerdown={(event) => startResize(event, colIndex)}
                ></button>
              {/if}
            </th>
          {/each}
          <th class="ledger-add-col-head" scope="col">
            <button
              type="button"
              class="ledger-add-col"
              {disabled}
              title="Add column"
              aria-label="Add column"
              onclick={addColumn}
            >
              <Plus size={14} strokeWidth={2} />
            </button>
          </th>
          <th class="ledger-row-actions-head" scope="col">
            <span class="sr-only">Row actions</span>
          </th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row, rowIndex (rowIndex)}
          <tr
            class="ledger-row border-b border-surface-500/20"
            class:ledger-row--selected={selectedRowIndexes.has(rowIndex)}
          >
            <td class="ledger-gutter">
              <button
                type="button"
                class="ledger-gutter-btn"
                {disabled}
                aria-label="Select row {rowIndex + 1}"
                aria-pressed={selectedRowIndexes.has(rowIndex)}
                onclick={(event) => selectRow(rowIndex, event)}
              >
                {rowIndex + 1}
              </button>
            </td>
            {#each columns as column, colIndex (colIndex)}
              <td
                class="px-0 py-0 {alignClass(column, colIndex)}"
                style={columnWidth(column, colIndex)
                  ? `width:${columnWidth(column, colIndex)};min-width:${columnWidth(column, colIndex)};max-width:${columnWidth(column, colIndex)}`
                  : undefined}
              >
                <input
                  class="ledger-cell {alignClass(column, colIndex)} {isNumericColumn(
                    column,
                    colIndex,
                  )
                    ? 'ledger-cell-amount font-mono tabular-nums'
                    : ''}"
                  type="text"
                  value={row[colIndex] ?? ""}
                  {disabled}
                  data-ledger-cell="{rowIndex}-{colIndex}"
                  onfocus={() => {
                    if (selectionAnchor != null && !selectedRowIndexes.has(rowIndex)) {
                      clearSelection();
                    }
                  }}
                  oninput={(event) =>
                    updateCell(
                      rowIndex,
                      colIndex,
                      (event.currentTarget as HTMLInputElement).value,
                    )}
                  onkeydown={(event) =>
                    handleCellKeydown(event, rowIndex, colIndex)}
                />
              </td>
            {/each}
            <td class="ledger-add-col-spacer" aria-hidden="true"></td>
            <td class="ledger-row-actions">
              {#if !disabled}
                <button
                  type="button"
                  class="ledger-row-remove"
                  title="Remove row"
                  aria-label="Remove row {rowIndex + 1}"
                  onclick={() => removeRow(rowIndex)}
                >
                  <X size={12} strokeWidth={2} />
                </button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>

    <button
      type="button"
      class="ledger-add-row"
      {disabled}
      onclick={() => addRow()}
    >
      <Plus size={13} strokeWidth={2.25} />
      Add row
    </button>
  </div>
</div>
