<script lang="ts">
  import VaultNotePicker from "$lib/components/vault/VaultNotePicker.svelte";
  import { getVaultNote } from "$lib/daemon";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import {
    findTableForView,
    resolveViewSourcePath,
    type MedousaViewQuery,
    type ViewPredicate,
  } from "$lib/utils/markdownView";
  import { columnDisplayLabel } from "$lib/utils/ledgerSheet";
  import { Plus, X } from "@lucide/svelte";

  interface Props {
    open: boolean;
    mode?: "insert" | "edit";
    initialQuery?: MedousaViewQuery | null;
    onSave: (query: MedousaViewQuery) => void;
    onClose: () => void;
  }

  let {
    open,
    mode = "insert",
    initialQuery = null,
    onSave,
    onClose,
  }: Props = $props();

  let fromPath = $state("");
  let tableMode = $state<"first" | "ledger">("first");
  let wheres = $state<ViewPredicate[]>([]);
  let sortColumn = $state("");
  let sortDescending = $state(false);
  let selectedColumns = $state<string[]>([]);
  let availableColumns = $state<string[]>([]);
  let loadingColumns = $state(false);
  let columnError = $state<string | null>(null);
  let notePickerOpen = $state(false);
  let filterColumn = $state("");
  let filterOp = $state<"=" | "!=">("!=");
  let filterValue = $state("");

  const fromLabel = $derived(
    fromPath
      ? vault.labelByPathMap.get(fromPath) ??
        vaultDisplayTitle(fromPath.split("/").pop()?.replace(/\.md$/i, "") ?? fromPath, fromPath)
      : "a note",
  );

  const canSave = $derived(Boolean(fromPath.trim()));
  const commitLabel = $derived(mode === "edit" ? "Save view" : "Insert view");

  $effect(() => {
    if (!open) return;
    const seed = initialQuery;
    const resolvedFrom = seed?.from
      ? resolveViewSourcePath(seed.from, vault.selectedPath, vault.notes) ??
        seed.from
      : "";
    fromPath = resolvedFrom;
    tableMode = seed?.table ?? "first";
    wheres = seed?.wheres ? [...seed.wheres] : [];
    sortColumn = seed?.sort?.column ?? "";
    sortDescending = seed?.sort?.descending ?? false;
    selectedColumns = seed?.columns ? [...seed.columns] : [];
    availableColumns = [];
    columnError = null;
    filterColumn = "";
    filterOp = "!=";
    filterValue = "";
    notePickerOpen = false;
  });

  $effect(() => {
    if (!open || !fromPath) {
      availableColumns = [];
      return;
    }
    const path = fromPath;
    const mode = tableMode;
    loadingColumns = true;
    columnError = null;
    void (async () => {
      try {
        const note = await getVaultNote(path);
        if (fromPath !== path) return;
        const table = findTableForView(note.content, mode);
        if (!table) {
          availableColumns = [];
          columnError =
            mode === "ledger"
              ? "No ledger table found in that note."
              : "No table found in that note.";
          return;
        }
        availableColumns = table.headers.map((header) => columnDisplayLabel(header));
        if (!filterColumn && availableColumns[0]) filterColumn = availableColumns[0];
        if (sortColumn && !availableColumns.includes(sortColumn)) sortColumn = "";
        selectedColumns = selectedColumns.filter((column) =>
          availableColumns.includes(column),
        );
      } catch (err) {
        if (fromPath !== path) return;
        availableColumns = [];
        columnError = err instanceof Error ? err.message : String(err);
      } finally {
        if (fromPath === path) loadingColumns = false;
      }
    })();
  });

  function pickNote(path: string) {
    fromPath = path;
    notePickerOpen = false;
  }

  function addFilter() {
    const column = filterColumn.trim() || availableColumns[0];
    const value = filterValue.trim();
    if (!column || !value) return;
    wheres = [...wheres, { column, op: filterOp, value }];
    filterValue = "";
  }

  function removeFilter(index: number) {
    wheres = wheres.filter((_, i) => i !== index);
  }

  function toggleColumn(column: string) {
    if (selectedColumns.includes(column)) {
      selectedColumns = selectedColumns.filter((entry) => entry !== column);
      return;
    }
    selectedColumns = [...selectedColumns, column];
  }

  function buildQuery(): MedousaViewQuery | null {
    if (!fromPath.trim()) return null;
    return {
      from: fromPath.trim(),
      table: tableMode,
      wheres,
      sort: sortColumn
        ? { column: sortColumn, descending: sortDescending }
        : undefined,
      columns: selectedColumns.length > 0 ? selectedColumns : undefined,
    };
  }

  function commit() {
    const query = buildQuery();
    if (!query) return;
    onSave(query);
    onClose();
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && !notePickerOpen) {
      event.preventDefault();
      onClose();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="view-builder-title"
    tabindex="-1"
    onkeydown={onSheetKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div class="vault-interact-sheet vault-compose-sheet vault-bridge-sheet">
      <header class="vault-interact-header vault-compose-header">
        <h3 id="view-builder-title" class="sr-only">Query view</h3>
        <button
          type="button"
          class="vault-interact-dismiss ml-auto"
          aria-label="Close"
          onclick={onClose}
        >
          <X size={14} strokeWidth={2} />
        </button>
      </header>

      <p class="vault-compose-sentence">
        Query
        <button
          type="button"
          class="vault-compose-em-btn"
          class:vault-compose-em-btn--open={tableMode === "ledger"}
          onclick={() => {
            tableMode = tableMode === "first" ? "ledger" : "first";
          }}
        >
          {tableMode === "ledger" ? "ledger table" : "first table"}
        </button>
        from
        <button
          type="button"
          class="vault-compose-em-btn"
          onclick={() => (notePickerOpen = true)}
        >
          {fromLabel}
        </button>
      </p>

      <div class="vault-chip-row" role="group" aria-label="Table type">
        <button
          type="button"
          class="vault-chip"
          class:vault-chip--active={tableMode === "first"}
          onclick={() => (tableMode = "first")}
        >
          First table
        </button>
        <button
          type="button"
          class="vault-chip"
          class:vault-chip--active={tableMode === "ledger"}
          onclick={() => (tableMode = "ledger")}
        >
          Ledger
        </button>
        <button
          type="button"
          class="vault-chip"
          onclick={() => (notePickerOpen = true)}
        >
          {fromPath ? "Change note" : "Pick note"}
        </button>
      </div>

      {#if loadingColumns}
        <p class="vault-interact-note">Reading columns…</p>
      {:else if columnError}
        <p class="vault-interact-note">{columnError}</p>
      {:else if availableColumns.length > 0}
        <div class="vault-bridge-section">
          <p class="vault-interact-kicker">Filter</p>
          <div class="ledger-filter-composer">
            <select class="ledger-filter-select" bind:value={filterColumn} aria-label="Filter column">
              {#each availableColumns as column (column)}
                <option value={column}>{column}</option>
              {/each}
            </select>
            <select class="ledger-filter-select ledger-filter-op" bind:value={filterOp} aria-label="Filter operator">
              <option value="!=">≠</option>
              <option value="=">=</option>
            </select>
            <input
              class="ledger-filter-input"
              type="text"
              placeholder="Value"
              bind:value={filterValue}
              onkeydown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  addFilter();
                }
              }}
            />
            <button
              type="button"
              class="ledger-filter-commit"
              disabled={!filterValue.trim()}
              aria-label="Add filter"
              onclick={addFilter}
            >
              <Plus size={14} strokeWidth={2} />
            </button>
          </div>
          {#if wheres.length > 0}
            <div class="ledger-filter-chips">
              {#each wheres as filter, index (filter.column + filter.op + filter.value + index)}
                <button
                  type="button"
                  class="ledger-filter-chip"
                  title="Remove filter"
                  onclick={() => removeFilter(index)}
                >
                  {filter.column} {filter.op} {filter.value}
                  <X size={11} strokeWidth={2} />
                </button>
              {/each}
            </div>
          {/if}
        </div>

        <div class="vault-bridge-section">
          <p class="vault-interact-kicker">Sort</p>
          <div class="vault-chip-row">
            <button
              type="button"
              class="vault-chip"
              class:vault-chip--active={!sortColumn}
              onclick={() => {
                sortColumn = "";
              }}
            >
              None
            </button>
            {#each availableColumns as column (column)}
              <button
                type="button"
                class="vault-chip"
                class:vault-chip--active={sortColumn === column}
                onclick={() => {
                  if (sortColumn === column) {
                    sortDescending = !sortDescending;
                    return;
                  }
                  sortColumn = column;
                  sortDescending = false;
                }}
              >
                {column}{sortColumn === column ? (sortDescending ? " ↓" : " ↑") : ""}
              </button>
            {/each}
          </div>
        </div>

        <div class="vault-bridge-section">
          <p class="vault-interact-kicker">Columns · all if none</p>
          <div class="vault-chip-row">
            {#each availableColumns as column (column)}
              <button
                type="button"
                class="vault-chip"
                class:vault-chip--active={selectedColumns.includes(column)}
                onclick={() => toggleColumn(column)}
              >
                {column}
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <div class="vault-compose-footer">
        <button
          type="button"
          class="vault-interact-commit"
          disabled={!canSave}
          onclick={commit}
        >
          {commitLabel}
        </button>
      </div>
    </div>
  </div>
{/if}

<VaultNotePicker
  open={notePickerOpen}
  onSelect={pickNote}
  onClose={() => (notePickerOpen = false)}
/>
