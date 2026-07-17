<script lang="ts">
  import { tick } from "svelte";
  import { Plus, X } from "@lucide/svelte";
  import {
    findFirstPipeTable,
    serializePipeTable,
  } from "$lib/utils/markdownTable";
  import { seedChartTableMarkdown } from "$lib/utils/vaultChartFence";
  import type { ChartFenceType } from "$lib/utils/liquidFenceTemplates";

  interface Props {
    content: string;
    chartType?: ChartFenceType;
    disabled?: boolean;
    onchange: (nextContent: string) => void;
  }

  let {
    content,
    chartType = "bar",
    disabled = false,
    onchange,
  }: Props = $props();

  let headers = $state<string[]>([]);
  let rows = $state<string[][]>([]);
  let syncedContent = $state("");
  let selectionAnchor = $state<number | null>(null);
  let selectionFocus = $state<number | null>(null);
  let focusRequest = $state<{ row: number; col: number } | null>(null);
  let headerFocusRequest = $state<number | null>(null);

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
    loadFromContent(content);
  });

  $effect(() => {
    const req = focusRequest;
    if (!req) return;
    void tick().then(() => {
      const el = document.querySelector(
        `[data-chart-cell="${req.row}-${req.col}"]`,
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
        `[data-chart-header="${col}"]`,
      ) as HTMLInputElement | null;
      el?.focus();
      el?.select();
      headerFocusRequest = null;
    });
  });

  function loadFromContent(raw: string) {
    const table = findFirstPipeTable(raw.trim());
    if (!table) {
      const seed = seedChartTableMarkdown(chartType);
      const seeded = findFirstPipeTable(seed);
      headers = seeded?.headers ?? ["Category", "Series"];
      rows = seeded?.rows?.length ? seeded.rows.map((r) => [...r]) : [["", ""]];
      const next = serializePipeTable(headers, rows);
      syncedContent = next;
      onchange(next);
      return;
    }
    headers = [...table.headers];
    rows =
      table.rows.length > 0
        ? table.rows.map((row) => headers.map((_, i) => row[i] ?? ""))
        : [headers.map(() => "")];
    // Keep parent string so re-serialize whitespace doesn't re-trigger load.
    syncedContent = raw;
  }

  function emit(nextHeaders: string[], nextRows: string[][]) {
    headers = nextHeaders;
    rows = nextRows;
    const next = serializePipeTable(nextHeaders, nextRows);
    syncedContent = next;
    onchange(next);
  }

  function emptyRow(columnCount = headers.length): string[] {
    return Array.from({ length: columnCount }, () => "");
  }

  function updateCell(rowIndex: number, colIndex: number, value: string) {
    const next = rows.map((row, ri) =>
      ri === rowIndex
        ? row.map((cell, ci) => (ci === colIndex ? value : cell))
        : [...row],
    );
    emit(headers, next);
  }

  function updateHeader(colIndex: number, value: string) {
    const nextHeaders = headers.map((h, i) => (i === colIndex ? value : h));
    emit(nextHeaders, rows);
  }

  function addRow(afterIndex?: number) {
    const next = rows.map((row) => [...row]);
    const row = emptyRow();
    if (afterIndex == null) {
      next.push(row);
      emit(headers, next);
      focusRequest = { row: next.length - 1, col: 0 };
      return;
    }
    next.splice(afterIndex + 1, 0, row);
    emit(headers, next);
    focusRequest = { row: afterIndex + 1, col: 0 };
  }

  function removeRows(indexes: number[]) {
    if (indexes.length === 0) return;
    const remove = new Set(indexes);
    let next = rows.filter((_, index) => !remove.has(index));
    if (next.length === 0) next = [emptyRow()];
    emit(headers, next);
    selectionAnchor = null;
    selectionFocus = null;
  }

  function removeRow(rowIndex: number) {
    removeRows([rowIndex]);
  }

  function addColumn() {
    const label = `Series ${headers.length}`;
    const nextHeaders = [...headers, label];
    const nextRows = rows.map((row) => [...row, ""]);
    emit(nextHeaders, nextRows);
    headerFocusRequest = nextHeaders.length - 1;
  }

  function removeColumn(colIndex: number) {
    if (headers.length <= 2) return;
    const nextHeaders = headers.filter((_, index) => index !== colIndex);
    const nextRows = rows.map((row) => row.filter((_, index) => index !== colIndex));
    emit(nextHeaders, nextRows);
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

  function focusCell(row: number, col: number) {
    const safeRow = Math.max(0, Math.min(rows.length - 1, row));
    const safeCol = Math.max(0, Math.min(headers.length - 1, col));
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
      if (rowIndex >= rows.length - 1) addRow();
      else focusCell(rowIndex + 1, colIndex);
      return;
    }

    if (event.key === "Tab") {
      event.preventDefault();
      if (event.shiftKey) {
        if (colIndex > 0) focusCell(rowIndex, colIndex - 1);
        else if (rowIndex > 0) focusCell(rowIndex - 1, headers.length - 1);
      } else if (colIndex < headers.length - 1) {
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
    if (event.key === "ArrowRight" && atEnd && colIndex < headers.length - 1) {
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

</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="ledger-sheet vault-chart-data-sheet-table"
  role="group"
  aria-label="Chart data table"
  onkeydown={handleSheetKeydown}
>
  <div class="overflow-x-auto">
    <table class="ledger-table">
      <thead>
        <tr class="ledger-header-row border-b border-surface-500/50 text-left">
          <th class="ledger-gutter-head" scope="col">
            <span class="sr-only">Row</span>
            #
          </th>
          {#each headers as header, colIndex (colIndex)}
            <th class="ledger-header-cell" scope="col">
              <div class="ledger-header-edit">
                <input
                  class="ledger-header-input"
                  type="text"
                  value={header}
                  {disabled}
                  data-chart-header={colIndex}
                  aria-label="Column {colIndex + 1} header"
                  oninput={(event) =>
                    updateHeader(
                      colIndex,
                      (event.currentTarget as HTMLInputElement).value,
                    )}
                />
                {#if !disabled && headers.length > 2}
                  <button
                    type="button"
                    class="ledger-col-remove"
                    title="Remove column"
                    aria-label="Remove column {header || colIndex + 1}"
                    onclick={() => removeColumn(colIndex)}
                  >
                    <X size={11} strokeWidth={2} />
                  </button>
                {/if}
              </div>
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
            {#each headers as _, colIndex (colIndex)}
              <td class="px-0 py-0">
                <input
                  class="ledger-cell"
                  type="text"
                  value={row[colIndex] ?? ""}
                  {disabled}
                  data-chart-cell="{rowIndex}-{colIndex}"
                  onfocus={() => {
                    if (
                      selectionAnchor != null &&
                      !selectedRowIndexes.has(rowIndex)
                    ) {
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
