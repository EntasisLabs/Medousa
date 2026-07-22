<script lang="ts">
  /**
   * Workbook marker surface — title + sheet list (not empty Live TipTap).
   */
  import { vault } from "$lib/stores/vault.svelte";
  import {
    parseWorkbookManifest,
    serializeWorkbookManifest,
    sheetStem,
    workbookFolderFromMarkerPath,
    type WorkbookManifest,
  } from "$lib/utils/workbook";

  interface Props {
    content: string;
    disabled?: boolean;
    onchange: (next: string) => void;
  }

  let { content, disabled = false, onchange }: Props = $props();

  const manifest = $derived.by((): WorkbookManifest => {
    return (
      parseWorkbookManifest(content) ?? {
        title: "Untitled workbook",
        sheets: ["Sheet1"],
      }
    );
  });

  const folder = $derived(
    vault.selectedPath
      ? workbookFolderFromMarkerPath(vault.selectedPath)
      : null,
  );

  function commit(next: WorkbookManifest) {
    if (disabled) return;
    onchange(serializeWorkbookManifest(next));
  }

  function openSheet(stem: string) {
    if (!folder && folder !== "") return;
    const path = folder ? `${folder}/${sheetStem(stem)}.md` : `${sheetStem(stem)}.md`;
    void vault.openNote(path);
  }
</script>

<div class="workbook-manifest flex min-h-0 flex-1 flex-col gap-4 px-8 py-7">
  <div class="flex flex-col gap-1">
    <label class="text-[0.65rem] font-semibold uppercase tracking-[0.08em] text-surface-500" for="wb-title">
      Workbook
    </label>
    <input
      id="wb-title"
      class="workbook-manifest__title"
      type="text"
      value={manifest.title}
      {disabled}
      onchange={(event) => {
        const title = (event.currentTarget as HTMLInputElement).value;
        commit({ ...manifest, title });
      }}
    />
  </div>

  <div class="flex flex-col gap-2">
    <div class="text-[0.65rem] font-semibold uppercase tracking-[0.08em] text-surface-500">
      Sheets
    </div>
    {#if manifest.sheets.length === 0}
      <p class="text-sm text-surface-400">No sheets listed yet.</p>
    {:else}
      <ul class="workbook-manifest__sheets">
        {#each manifest.sheets as stem (stem)}
          <li>
            <button
              type="button"
              class="workbook-manifest__sheet"
              {disabled}
              onclick={() => openSheet(stem)}
            >
              {stem}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <p class="text-xs text-surface-500">
    Marker note for this folder. Use Raw to edit frontmatter; open a sheet to edit the grid.
  </p>
</div>

<style>
  .workbook-manifest__title {
    width: 100%;
    max-width: 28rem;
    border: none;
    border-bottom: 1px solid color-mix(in srgb, currentColor 18%, transparent);
    background: transparent;
    color: rgb(var(--color-surface-50));
    font-size: 1.35rem;
    font-weight: 600;
    letter-spacing: -0.02em;
    padding: 0.25rem 0;
  }
  .workbook-manifest__title:focus {
    outline: none;
    border-bottom-color: rgb(var(--color-primary-400));
  }
  .workbook-manifest__sheets {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    max-width: 28rem;
  }
  .workbook-manifest__sheet {
    display: block;
    width: 100%;
    text-align: left;
    padding: 0.55rem 0.75rem;
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
    background: color-mix(in srgb, currentColor 4%, transparent);
    color: rgb(var(--color-surface-100));
    font-size: 0.9rem;
    cursor: pointer;
  }
  .workbook-manifest__sheet:hover:not(:disabled) {
    background: color-mix(in srgb, currentColor 8%, transparent);
  }
</style>
