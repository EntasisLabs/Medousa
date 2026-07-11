<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import {
    ArrowDown,
    ArrowUp,
    Check,
    Copy,
    Filter,
    Plus,
    SlidersHorizontal,
    X,
  } from "@lucide/svelte";
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
    type LedgerColumnAlign,
    type LedgerColumnType,
  } from "$lib/utils/ledgerSheet";
  import {
    applyMedousaSheetView,
    emptyMedousaSheetConfig,
    isMedousaSheetConfigEmpty,
    ledgerSheetConfigFromContent,
    upsertLedgerSheetFence,
    type MedousaSheetConfig,
  } from "$lib/utils/medousaSheet";
  import {
    MARKDOWN_COLOR_OPTIONS,
    MARKDOWN_COLOR_HEX,
    type MarkdownColorId,
  } from "$lib/utils/vaultMarkdownColors";

  interface Props {
    content: string;
    disabled?: boolean;
    onchange: (nextContent: string) => void;
  }

  let { content, disabled = false, onchange }: Props = $props();

  let columns = $state<LedgerColumn[]>([]);
  let rows = $state<string[][]>([]);
  let sheet = $state<MedousaSheetConfig>(emptyMedousaSheetConfig());
  let syncedContent = $state("");
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  let selectionAnchor = $state<number | null>(null);
  let selectionFocus = $state<number | null>(null);
  let focusRequest = $state<{ row: number; col: number } | null>(null);
  let headerFocusRequest = $state<number | null>(null);
  let resizeDraftWidth = $state<string | null>(null);
  let resizingCol = $state<number | null>(null);
  let filterColumn = $state("");
  let filterOp = $state<"=" | "!=">("!=");
  let filterValue = $state("");
  let filterOpen = $state(false);
  let renamingCol = $state<number | null>(null);
  let metaCol = $state<number | null>(null);
  let headerSortTimer: ReturnType<typeof setTimeout> | null = null;

  const protectedColumns = $derived(
    ledgerProtectedColumnIndexes(columns.map((column) => column.label)),
  );

  const viewRows = $derived(applyMedousaSheetView(columns, rows, sheet));

  const sheetActive = $derived(!isMedousaSheetConfigEmpty(sheet));

  const viewBadgeCount = $derived(
    sheet.filters.length + (sheet.sort ? 1 : 0),
  );

  const canAddFilter = $derived(filterValue.trim().length > 0);

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
    sheet = ledgerSheetConfigFromContent(content);
    filterColumn = columns[0]?.label ?? "";
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
    renamingCol = col;
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
    if (headerSortTimer) clearTimeout(headerSortTimer);
    detachResizeListeners();
  });

  function persistMarkdown(
    nextColumns: LedgerColumn[],
    nextRows: string[][],
    nextSheet: MedousaSheetConfig,
  ) {
    columns = nextColumns;
    rows = nextRows;
    sheet = nextSheet;
    const base = syncedContent || content;
    let updated = replaceLedgerTable(
      base,
      nextRows,
      serializeLedgerColumns(nextColumns),
    );
    if (!updated) return;
    updated = upsertLedgerSheetFence(updated, nextSheet);
    syncedContent = updated;
    onchange(updated);
  }

  function emitSheet(nextColumns: LedgerColumn[], nextRows: string[][]) {
    persistMarkdown(nextColumns, nextRows, sheet);
  }

  function emitSheetConfig(nextSheet: MedousaSheetConfig) {
    persistMarkdown(columns, rows, nextSheet);
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

  function updateColumnMeta(
    colIndex: number,
    patch: {
      type?: LedgerColumnType | "";
      align?: LedgerColumnAlign | "";
      color?: MarkdownColorId | "";
    },
  ) {
    const nextColumns = columns.map((column, index) =>
      index === colIndex ? mergeColumnMeta(column, patch) : column,
    );
    emitSheet(nextColumns, rows);
  }

  function toggleMetaPanel(colIndex: number) {
    metaCol = metaCol === colIndex ? null : colIndex;
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

  function toggleFilterOpen() {
    filterOpen = !filterOpen;
    if (filterOpen) {
      if (!filterColumn) filterColumn = columns[0]?.label ?? "";
      void tick().then(() => {
        const el = document.querySelector(
          "[data-ledger-filter-value]",
        ) as HTMLInputElement | null;
        el?.focus();
      });
    }
  }

  function addFilter() {
    const column = filterColumn.trim() || columns[0]?.label;
    const value = filterValue.trim();
    if (!column || !value) return;
    emitSheetConfig({
      ...sheet,
      filters: [...sheet.filters, { column, op: filterOp, value }],
    });
    filterValue = "";
    filterOpen = true;
  }

  function removeFilter(index: number) {
    emitSheetConfig({
      ...sheet,
      filters: sheet.filters.filter((_, i) => i !== index),
    });
  }

  function setSort(column: string) {
    if (sheet.sort?.column === column) {
      if (sheet.sort.descending) {
        emitSheetConfig({
          ...sheet,
          sort: { column, descending: false },
        });
        return;
      }
      emitSheetConfig({
        ...sheet,
        sort: undefined,
      });
      return;
    }
    emitSheetConfig({
      ...sheet,
      sort: { column, descending: true },
    });
  }

  function beginRename(colIndex: number) {
    if (disabled) return;
    if (headerSortTimer) {
      clearTimeout(headerSortTimer);
      headerSortTimer = null;
    }
    renamingCol = colIndex;
    headerFocusRequest = colIndex;
  }

  function finishRename() {
    renamingCol = null;
  }

  function onHeaderSortClick(column: string) {
    if (disabled) return;
    if (headerSortTimer) clearTimeout(headerSortTimer);
    headerSortTimer = setTimeout(() => {
      headerSortTimer = null;
      setSort(column);
    }, 220);
  }

  function clearSheetView() {
    filterValue = "";
    filterOpen = false;
    emitSheetConfig(emptyMedousaSheetConfig());
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
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement
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
  <div class="ledger-toolbar flex shrink-0 flex-wrap items-center gap-2 border-b border-surface-500/40 px-4 py-2">
    <div class="ledger-sheet-controls min-w-0 flex-1">
      <div class="ledger-filter-row">
        <button
          type="button"
          class="ledger-filter-toggle"
          class:ledger-filter-toggle--open={filterOpen}
          class:ledger-filter-toggle--active={sheet.filters.length > 0}
          {disabled}
          aria-expanded={filterOpen}
          aria-label="Filter"
          title="Filter"
          onclick={toggleFilterOpen}
        >
          <Filter size={13} strokeWidth={2} />
          <span>Filter</span>
          {#if viewBadgeCount > 0}
            <span class="ledger-filter-badge">{viewBadgeCount}</span>
          {/if}
        </button>

        {#if filterOpen}
          <div class="ledger-filter-composer">
            <select
              class="ledger-filter-select"
              {disabled}
              bind:value={filterColumn}
              aria-label="Filter column"
            >
              {#each columns as column (column.label)}
                <option value={column.label}>{column.label}</option>
              {/each}
            </select>
            <select
              class="ledger-filter-select ledger-filter-op"
              {disabled}
              bind:value={filterOp}
              aria-label="Filter operator"
            >
              <option value="!=">≠</option>
              <option value="=">=</option>
            </select>
            <input
              class="ledger-filter-input"
              type="text"
              placeholder="Value"
              {disabled}
              data-ledger-filter-value
              bind:value={filterValue}
              onkeydown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  addFilter();
                }
                if (event.key === "Escape") {
                  event.preventDefault();
                  filterOpen = false;
                }
              }}
            />
            <button
              type="button"
              class="ledger-filter-commit"
              disabled={disabled || !canAddFilter}
              title="Add filter"
              aria-label="Add filter"
              onclick={addFilter}
            >
              <Plus size={14} strokeWidth={2} />
            </button>
          </div>
        {/if}
      </div>

      {#if sheet.filters.length > 0 || sheet.sort}
        <div class="ledger-filter-chips">
          {#each sheet.filters as filter, index (filter.column + filter.op + filter.value + index)}
            <button
              type="button"
              class="ledger-filter-chip"
              {disabled}
              title="Remove filter"
              onclick={() => removeFilter(index)}
            >
              {filter.column} {filter.op} {filter.value}
              <X size={11} strokeWidth={2} />
            </button>
          {/each}
          {#if sheet.sort}
            <button
              type="button"
              class="ledger-filter-chip ledger-filter-chip--sort"
              {disabled}
              title="Clear sort"
              onclick={() =>
                emitSheetConfig({
                  ...sheet,
                  sort: undefined,
                })}
            >
              {#if sheet.sort.descending}
                <ArrowDown size={11} strokeWidth={2} />
              {:else}
                <ArrowUp size={11} strokeWidth={2} />
              {/if}
              {sheet.sort.column}
              <X size={11} strokeWidth={2} />
            </button>
          {/if}
          {#if sheetActive}
            <button
              type="button"
              class="ledger-filter-clear"
              {disabled}
              onclick={clearSheetView}
            >
              Clear view
            </button>
          {/if}
        </div>
      {/if}
    </div>

    <button
      type="button"
      class="ledger-copy-csv"
      {disabled}
      onclick={copyCsv}
    >
      {#if copied}
        <Check size={13} strokeWidth={2} />
        Copied
      {:else}
        <Copy size={13} strokeWidth={2} />
        Copy CSV
      {/if}
    </button>
  </div>

  {#if sheetActive}
    <p class="ledger-view-status">
      Showing {viewRows.length} of {rows.length}
      {#if sheet.title}
        · {sheet.title}
      {/if}
    </p>
  {/if}

  <div class="min-h-0 flex-1 overflow-auto p-4 pt-2">
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
              class:ledger-header-cell--sorted={sheet.sort?.column === column.label}
              scope="col"
              style={columnStyle(column, colIndex)}
            >
              <div class="ledger-header-edit">
                {#if renamingCol === colIndex}
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
                    onblur={finishRename}
                    onkeydown={(event) => {
                      if (event.key === "Enter" || event.key === "Escape") {
                        event.preventDefault();
                        (event.currentTarget as HTMLInputElement).blur();
                      }
                    }}
                  />
                {:else}
                  <button
                    type="button"
                    class="ledger-header-sort {alignClass(column, colIndex)}"
                    class:ledger-header-sort--active={sheet.sort?.column ===
                      column.label}
                    {disabled}
                    title="Sort by {column.label} · Double-click to rename"
                    aria-label="Sort by {column.label}"
                    onclick={() => onHeaderSortClick(column.label)}
                    ondblclick={(event) => {
                      event.preventDefault();
                      beginRename(colIndex);
                    }}
                  >
                    <span class="ledger-header-sort-label">{column.label}</span>
                    {#if sheet.sort?.column === column.label}
                      {#if sheet.sort.descending}
                        <ArrowDown size={11} strokeWidth={2} />
                      {:else}
                        <ArrowUp size={11} strokeWidth={2} />
                      {/if}
                    {/if}
                  </button>
                {/if}
                {#if !disabled}
                  <button
                    type="button"
                    class="ledger-col-meta-btn"
                    class:ledger-col-meta-btn--open={metaCol === colIndex}
                    title="Column format"
                    aria-label="Format column {column.label}"
                    aria-expanded={metaCol === colIndex}
                    onclick={(event) => {
                      event.stopPropagation();
                      toggleMetaPanel(colIndex);
                    }}
                  >
                    <SlidersHorizontal size={11} strokeWidth={2} />
                  </button>
                {/if}
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
                {#if metaCol === colIndex}
                  <div
                    class="ledger-col-meta-panel"
                    role="dialog"
                    aria-label="Column format"
                  >
                    <p class="ledger-col-meta-label">Type</p>
                    <div class="ledger-col-meta-row">
                      {#each ["text", "date", "currency", "number"] as type (type)}
                        <button
                          type="button"
                          class="vault-chip"
                          class:vault-chip--active={(column.meta.type ??
                            resolveColumnType(column, colIndex)) === type}
                          onclick={() =>
                            updateColumnMeta(colIndex, {
                              type: type as LedgerColumnType,
                            })}
                        >
                          {type}
                        </button>
                      {/each}
                    </div>
                    <p class="ledger-col-meta-label">Align</p>
                    <div class="ledger-col-meta-row">
                      {#each ["left", "center", "right"] as align (align)}
                        <button
                          type="button"
                          class="vault-chip"
                          class:vault-chip--active={resolveColumnAlign(
                            column,
                            colIndex,
                          ) === align}
                          onclick={() =>
                            updateColumnMeta(colIndex, {
                              align: align as LedgerColumnAlign,
                            })}
                        >
                          {align}
                        </button>
                      {/each}
                    </div>
                    <p class="ledger-col-meta-label">Color</p>
                    <div class="ledger-col-meta-row">
                      <button
                        type="button"
                        class="vault-chip"
                        class:vault-chip--active={!column.meta.color}
                        onclick={() => updateColumnMeta(colIndex, { color: "" })}
                      >
                        None
                      </button>
                      {#each MARKDOWN_COLOR_OPTIONS as color (color.id)}
                        <button
                          type="button"
                          class="ledger-col-swatch"
                          class:ledger-col-swatch--active={column.meta.color ===
                            color.id}
                          style="--swatch:{color.swatch}"
                          title={color.label}
                          aria-label={color.label}
                          onclick={() =>
                            updateColumnMeta(colIndex, { color: color.id })}
                        ></button>
                      {/each}
                    </div>
                    <button
                      type="button"
                      class="ledger-filter-clear"
                      onclick={() => (metaCol = null)}
                    >
                      Done
                    </button>
                  </div>
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
        {#each viewRows as viewRow, viewIndex (viewRow.sourceIndex)}
          {@const row = viewRow.cells}
          {@const rowIndex = viewRow.sourceIndex}
          <tr
            class="ledger-row border-b border-surface-500/20"
            class:ledger-row--selected={selectedRowIndexes.has(rowIndex)}
          >
            <td class="ledger-gutter">
              <button
                type="button"
                class="ledger-gutter-btn"
                {disabled}
                aria-label="Select row {viewIndex + 1}"
                aria-pressed={selectedRowIndexes.has(rowIndex)}
                onclick={(event) => selectRow(rowIndex, event)}
              >
                {viewIndex + 1}
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
                  aria-label="Remove row {viewIndex + 1}"
                  onclick={() => removeRow(rowIndex)}
                >
                  <X size={12} strokeWidth={2} />
                </button>
              {/if}
            </td>
          </tr>
        {:else}
          <tr>
            <td
              class="ledger-empty-view"
              colspan={columns.length + 3}
            >
              No rows match this view
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
