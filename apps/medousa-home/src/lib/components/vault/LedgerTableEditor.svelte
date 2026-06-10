<script lang="ts">
  import {
    findLedgerTable,
    ledgerCsvFromContent,
    ledgerRowsFromContent,
    replaceLedgerTable,
  } from "$lib/utils/markdownTable";

  interface Props {
    content: string;
    disabled?: boolean;
    onchange: (nextContent: string) => void;
  }

  let { content, disabled = false, onchange }: Props = $props();

  const table = $derived(findLedgerTable(content));
  const headers = $derived(
    table?.headers ?? ["Date", "Payee", "Amount", "Category"],
  );

  let rows = $state<string[][]>([]);
  let syncedContent = $state("");

  $effect(() => {
    if (content === syncedContent) return;
    rows = ledgerRowsFromContent(content);
    syncedContent = content;
  });

  function emitRows(nextRows: string[][]) {
    rows = nextRows;
    const updated = replaceLedgerTable(content, nextRows);
    if (updated) {
      syncedContent = updated;
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

  async function copyCsv() {
    const csv = ledgerCsvFromContent(content);
    if (!csv) return;
    try {
      await navigator.clipboard.writeText(csv);
    } catch {
      // Clipboard may be unavailable in Tauri webview.
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <div class="flex shrink-0 items-center justify-between gap-2 border-b border-surface-500/40 px-4 py-2">
    <p class="text-xs text-surface-400">Ledger table</p>
    <button
      type="button"
      class="btn btn-sm variant-ghost-surface"
      {disabled}
      onclick={copyCsv}
    >
      Copy CSV
    </button>
  </div>

  <div class="min-h-0 flex-1 overflow-auto p-4">
    <table class="w-full min-w-[520px] border-collapse text-sm">
      <thead>
        <tr class="border-b border-surface-500/50 text-left text-xs uppercase tracking-wide text-surface-400">
          {#each headers as header, colIndex (header + colIndex)}
            <th class="px-2 py-2 font-medium">{header}</th>
          {/each}
          <th class="w-10 px-2 py-2"></th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row, rowIndex (rowIndex)}
          <tr class="border-b border-surface-500/30">
            {#each headers as _, colIndex (colIndex)}
              <td class="px-1 py-1">
                <input
                  class="input w-full px-2 py-1 text-sm"
                  type="text"
                  value={row[colIndex] ?? ""}
                  {disabled}
                  oninput={(event) =>
                    updateCell(
                      rowIndex,
                      colIndex,
                      (event.currentTarget as HTMLInputElement).value,
                    )}
                />
              </td>
            {/each}
            <td class="px-1 py-1 text-right">
              <button
                type="button"
                class="btn btn-sm variant-ghost-surface px-2"
                {disabled}
                title="Remove row"
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
