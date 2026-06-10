<script lang="ts">
  import type { SpreadsheetPreviewData } from "$lib/utils/spreadsheetPreview";

  interface Props {
    data: SpreadsheetPreviewData;
  }

  let { data }: Props = $props();
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <div class="shrink-0 border-b border-surface-500/35 px-4 py-2">
    <p class="text-xs text-surface-400">
      {#if data.sheetName}
        Sheet: {data.sheetName}
      {:else}
        Spreadsheet preview
      {/if}
      · read-only · no formulas
    </p>
  </div>

  <div class="min-h-0 flex-1 overflow-auto p-4">
    {#if data.rows.length === 0 && data.totalRows === 0}
      <p class="text-sm text-surface-500">This sheet is empty.</p>
    {:else}
      <table class="w-full min-w-[520px] border-collapse text-sm">
        <thead>
          <tr
            class="sticky top-0 z-10 border-b border-surface-500/50 bg-surface-950/95 text-left text-xs uppercase tracking-wide text-surface-400"
          >
            {#each data.headers as header, colIndex (header + colIndex)}
              <th class="px-2 py-2 font-medium">{header}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each data.rows as row, rowIndex (rowIndex)}
            <tr class="border-b border-surface-500/30 hover:bg-surface-800/40">
              {#each data.headers as _, colIndex (colIndex)}
                <td class="max-w-[240px] truncate px-2 py-1.5 text-surface-100" title={row[colIndex] ?? ""}>
                  {row[colIndex] ?? ""}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    {#if data.truncated}
      <p class="mt-3 text-xs text-surface-500">
        Showing first {data.rows.length} of {data.totalRows} rows.
      </p>
    {/if}
  </div>
</div>
