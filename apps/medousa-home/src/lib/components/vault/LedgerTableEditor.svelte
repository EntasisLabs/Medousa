<script lang="ts">
  import { Check, Copy } from "@lucide/svelte";
  import {
    findLedgerTable,
    ledgerCsvFromContent,
    ledgerRowsFromContent,
    replaceLedgerTable,
  } from "$lib/utils/markdownTable";

  interface Props {
    content: string;
    disabled?: boolean;
    ledgerEditMode?: "table" | "raw";
    onToggleMode?: () => void;
    onchange: (nextContent: string) => void;
  }

  let {
    content,
    disabled = false,
    ledgerEditMode = "table",
    onToggleMode,
    onchange,
  }: Props = $props();

  const table = $derived(findLedgerTable(content));
  const headers = $derived(
    table?.headers ?? ["Date", "Payee", "Amount", "Category"],
  );

  let rows = $state<string[][]>([]);
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    rows = ledgerRowsFromContent(content);
  });

  function emitRows(nextRows: string[][]) {
    rows = nextRows;
    const updated = replaceLedgerTable(content, nextRows);
    if (updated) {
      onchange(updated);
    }
  }

  function updateCell(rowIndex: number, colIndex: number, value: string) {
    const next = rows.map((row, ri) =>
      ri === rowIndex
        ? row.map((cell, ci) => (ci === colIndex ? value : cell))
        : row,
    );
    emitRows(next);
  }

  function addRow() {
    emitRows([...rows, headers.map(() => "")]);
  }

  function removeRow(rowIndex: number) {
    if (rows.length <= 1) {
      emitRows([headers.map(() => "")]);
      return;
    }
    emitRows(rows.filter((_, index) => index !== rowIndex));
  }

  function handleCellKeydown(
    event: KeyboardEvent,
    rowIndex: number,
    colIndex: number,
  ) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (rowIndex === rows.length - 1) {
        addRow();
      }
    }
  }

  function isAmountColumn(header: string, colIndex: number): boolean {
    const lower = header.toLowerCase();
    return lower.includes("amount") || lower.includes("total") || colIndex === 2;
  }

  function isDateColumn(header: string, colIndex: number): boolean {
    const lower = header.toLowerCase();
    return lower.includes("date") || colIndex === 0;
  }

  async function copyCsv() {
    const csv = ledgerCsvFromContent(content);
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

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <div class="ledger-toolbar flex shrink-0 items-center justify-between gap-2 border-b border-surface-500/40 px-4 py-2">
    <div class="flex items-center gap-2">
      <p class="text-xs font-medium text-surface-300">Ledger</p>
      {#if onToggleMode}
        <div class="ledger-mode-toggle" role="group" aria-label="Ledger edit mode">
          <button
            type="button"
            class="ledger-mode-btn {ledgerEditMode === 'table' ? 'ledger-mode-btn-active' : ''}"
            {disabled}
            onclick={() => {
              if (ledgerEditMode !== "table") onToggleMode();
            }}
          >
            Table
          </button>
          <button
            type="button"
            class="ledger-mode-btn {ledgerEditMode === 'raw' ? 'ledger-mode-btn-active' : ''}"
            {disabled}
            onclick={() => {
              if (ledgerEditMode !== "raw") onToggleMode();
            }}
          >
            Raw
          </button>
        </div>
      {/if}
    </div>
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
    <table class="ledger-table w-full min-w-[520px] border-collapse text-sm">
      <thead>
        <tr class="border-b border-surface-500/50 text-left text-[11px] uppercase tracking-wide text-surface-500">
          {#each headers as header, colIndex (header + colIndex)}
            <th
              class="px-2 py-2 font-medium {isAmountColumn(header, colIndex)
                ? 'text-right'
                : ''} {isDateColumn(header, colIndex) ? 'w-28' : ''}"
            >
              {header}
            </th>
          {/each}
          <th class="w-8 px-1 py-2"></th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row, rowIndex (rowIndex)}
          <tr class="ledger-row border-b border-surface-500/20">
            {#each headers as header, colIndex (colIndex)}
              <td
                class="px-0 py-0 {isAmountColumn(header, colIndex)
                  ? 'text-right'
                  : ''}"
              >
                <input
                  class="ledger-cell {isAmountColumn(header, colIndex)
                    ? 'ledger-cell-amount text-right font-mono tabular-nums'
                    : ''}"
                  type="text"
                  value={row[colIndex] ?? ""}
                  {disabled}
                  oninput={(event) =>
                    updateCell(
                      rowIndex,
                      colIndex,
                      (event.currentTarget as HTMLInputElement).value,
                    )}
                  onkeydown={(event) => handleCellKeydown(event, rowIndex, colIndex)}
                />
              </td>
            {/each}
            <td class="px-1 py-1 text-right">
              <button
                type="button"
                class="ledger-row-remove"
                {disabled}
                title="Remove row"
                aria-label="Remove row"
                onclick={() => removeRow(rowIndex)}
              >
                ×
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>

    <button
      type="button"
      class="btn btn-sm variant-soft-surface mt-3"
      {disabled}
      onclick={addRow}
    >
      Add row
    </button>
  </div>
</div>
