<script lang="ts">
  import "$lib/styles/files-surface.postcss";
  import { Table2 } from "@lucide/svelte";
  import type { SpreadsheetPreviewData } from "$lib/utils/spreadsheetPreview";

  interface Props {
    data: SpreadsheetPreviewData;
  }

  let { data }: Props = $props();
</script>

<div class="files-sheet flex min-h-0 flex-1 flex-col overflow-hidden">
  <div class="files-document-toolbar">
    <span class="files-save-state"><Table2 size={13} />{data.sheetName || 'Spreadsheet'}<span class="files-format-badge">Read-only</span></span>
    <span class="files-save-state">{data.totalRows} rows · {data.headers.length} columns</span>
  </div>

  <div class="files-sheet-body min-h-0 flex-1 overflow-auto">
    {#if data.rows.length === 0 && data.totalRows === 0}
      <div class="files-empty"><span class="files-empty-icon"><Table2 size={22} /></span><h3>This sheet is empty</h3><p>There are no cells to preview.</p></div>
    {:else}
      <table class="w-full min-w-[520px] border-collapse">
        <thead>
          <tr
            class="sticky top-0 z-10 text-left"
          >
            {#each data.headers as header, colIndex (header + colIndex)}
              <th>{header}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each data.rows as row, rowIndex (rowIndex)}
            <tr>
              {#each data.headers as _, colIndex (colIndex)}
                <td class="max-w-[240px] truncate" title={row[colIndex] ?? ""}>
                  {row[colIndex] ?? ""}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    {#if data.truncated}
      <p class="mt-3 text-xs text-content-quiet">
        Showing first {data.rows.length} of {data.totalRows} rows.
      </p>
    {/if}
  </div>
</div>
