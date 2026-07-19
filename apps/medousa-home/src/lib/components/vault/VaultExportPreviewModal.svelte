<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { X } from "@lucide/svelte";
  import {
    renderVaultNotePdfBlob,
    saveVaultNotePdfBlob,
    vaultPdfFilename,
  } from "$lib/utils/vaultPdfExport";
  import {
    renderVaultNoteDocxBlob,
    saveVaultNoteDocxBlob,
    vaultDocxFilename,
  } from "$lib/utils/vaultDocxExport";
  import {
    DEFAULT_VAULT_EXPORT_OPTIONS,
    normalizeVaultExportOptions,
    readVaultExportOptions,
    writeVaultExportOptions,
    type VaultExportFontFamily,
    type VaultExportFormat,
    type VaultExportMargins,
    type VaultExportOptions,
    type VaultExportOrientation,
    type VaultExportPageSize,
  } from "$lib/utils/vaultExportOptions";

  interface Props {
    open: boolean;
    title: string;
    content: string;
    labelByPath: Map<string, string>;
    notePath?: string | null;
    /** Initial format when opening the sheet. */
    initialFormat?: VaultExportFormat;
    onClose: () => void;
    /** True while a blob is being rendered (for overflow menu labels). */
    onPreparingChange?: (preparing: boolean) => void;
  }

  let {
    open,
    title,
    content,
    labelByPath,
    notePath = null,
    initialFormat = "pdf",
    onClose,
    onPreparingChange,
  }: Props = $props();

  let format = $state<VaultExportFormat>("pdf");
  let options = $state<VaultExportOptions>({ ...DEFAULT_VAULT_EXPORT_OPTIONS });
  let blob = $state<Blob | null>(null);
  let blobUrl = $state<string | null>(null);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let renderGen = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let wasOpen = false;

  function revokeBlobUrl() {
    if (blobUrl) {
      URL.revokeObjectURL(blobUrl);
      blobUrl = null;
    }
    blob = null;
  }

  function setPreparing(next: boolean) {
    loading = next;
    onPreparingChange?.(next);
  }

  function close() {
    renderGen += 1;
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    untrack(() => {
      revokeBlobUrl();
      error = null;
      saving = false;
      setPreparing(false);
    });
    onClose();
  }

  function persistOptions(next: VaultExportOptions) {
    options = next;
    writeVaultExportOptions(next);
  }

  function patchOptions(partial: Partial<VaultExportOptions>) {
    persistOptions(normalizeVaultExportOptions({ ...options, ...partial }));
  }

  async function renderNow(
    noteTitle: string,
    noteContent: string,
    labels: Map<string, string>,
    path: string | null,
    fmt: VaultExportFormat,
    opts: VaultExportOptions,
    gen: number,
  ) {
    setPreparing(true);
    error = null;
    revokeBlobUrl();
    try {
      if (fmt === "pdf") {
        const next = await renderVaultNotePdfBlob({
          title: noteTitle,
          content: noteContent,
          labelByPath: labels,
          notePath: path,
          exportOptions: opts,
        });
        if (gen !== renderGen) return;
        blob = next;
        blobUrl = URL.createObjectURL(next);
      } else {
        const next = await renderVaultNoteDocxBlob({
          title: noteTitle,
          content: noteContent,
          labelByPath: labels,
          notePath: path,
          exportOptions: opts,
        });
        if (gen !== renderGen) return;
        blob = next;
        // Word: no iframe preview — keep blob for save only
        blobUrl = null;
      }
    } catch (err) {
      if (gen !== renderGen) return;
      error = err instanceof Error ? err.message : String(err);
    } finally {
      if (gen === renderGen) setPreparing(false);
    }
  }

  function scheduleRender() {
    if (!open) return;
    if (debounceTimer) clearTimeout(debounceTimer);
    const noteTitle = title;
    const noteContent = content;
    const labels = labelByPath;
    const path = notePath;
    const fmt = format;
    const opts = options;
    const gen = ++renderGen;
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      void renderNow(noteTitle, noteContent, labels, path, fmt, opts, gen);
    }, 280);
  }

  $effect(() => {
    if (!open) {
      untrack(() => {
        wasOpen = false;
        renderGen += 1;
        if (debounceTimer) {
          clearTimeout(debounceTimer);
          debounceTimer = null;
        }
        revokeBlobUrl();
        error = null;
        setPreparing(false);
      });
      return;
    }

    // Load prefs + format only on open transition
    if (!wasOpen) {
      untrack(() => {
        wasOpen = true;
        options = readVaultExportOptions();
        format = initialFormat;
      });
    }

    // Depend on inputs so content/format/options changes re-render
    void title;
    void content;
    void labelByPath;
    void notePath;
    void format;
    void options.fontFamily;
    void options.baseFontPx;
    void options.pageSize;
    void options.orientation;
    void options.margins;
    void options.breakBeforeH2;
    void options.keepTogether;

    scheduleRender();
  });

  onDestroy(() => {
    renderGen += 1;
    if (debounceTimer) clearTimeout(debounceTimer);
    untrack(() => {
      revokeBlobUrl();
      onPreparingChange?.(false);
    });
  });

  async function handleSave() {
    if (!blob || saving) return;
    saving = true;
    error = null;
    try {
      const filename =
        format === "pdf" ? vaultPdfFilename(title) : vaultDocxFilename(title);
      const saved =
        format === "pdf"
          ? await saveVaultNotePdfBlob(blob, filename)
          : await saveVaultNoteDocxBlob(blob, filename);
      if (saved) close();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="vault-export-preview-title"
    tabindex="-1"
    onkeydown={onSheetKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) close();
    }}
  >
    <div class="vault-interact-sheet vault-export-preview-sheet">
      <header class="vault-interact-header vault-export-preview-header">
        <div class="min-w-0">
          <p class="vault-interact-kicker">Export preview</p>
          <h3
            id="vault-export-preview-title"
            class="truncate text-sm font-semibold text-surface-50"
          >
            {title}
          </h3>
        </div>
        <button
          type="button"
          class="vault-interact-dismiss shrink-0"
          aria-label="Close"
          onclick={close}
        >
          <X size={14} strokeWidth={2} />
        </button>
      </header>

      <div class="vault-export-preview-layout">
        <aside class="vault-export-preview-settings" aria-label="Export settings">
          <div class="vault-export-field">
            <span class="vault-export-label">Format</span>
            <div class="vault-export-seg">
              <button
                type="button"
                class="vault-export-seg__btn"
                class:vault-export-seg__btn--on={format === "pdf"}
                onclick={() => {
                  format = "pdf";
                }}
              >
                PDF
              </button>
              <button
                type="button"
                class="vault-export-seg__btn"
                class:vault-export-seg__btn--on={format === "docx"}
                onclick={() => {
                  format = "docx";
                }}
              >
                Word
              </button>
            </div>
          </div>

          <label class="vault-export-field">
            <span class="vault-export-label">Font</span>
            <select
              class="vault-export-select"
              value={options.fontFamily}
              onchange={(e) =>
                patchOptions({
                  fontFamily: (e.currentTarget.value || "system") as VaultExportFontFamily,
                })}
            >
              <option value="system">System sans</option>
              <option value="serif">Serif</option>
              <option value="mono">Mono</option>
            </select>
          </label>

          <label class="vault-export-field">
            <span class="vault-export-label">Size ({options.baseFontPx}px)</span>
            <input
              class="vault-export-range"
              type="range"
              min="11"
              max="16"
              step="1"
              value={options.baseFontPx}
              oninput={(e) =>
                patchOptions({ baseFontPx: Number(e.currentTarget.value) })}
            />
          </label>

          <label class="vault-export-field">
            <span class="vault-export-label">Page</span>
            <select
              class="vault-export-select"
              value={options.pageSize}
              onchange={(e) =>
                patchOptions({
                  pageSize: (e.currentTarget.value || "letter") as VaultExportPageSize,
                })}
            >
              <option value="letter">Letter</option>
              <option value="a4">A4</option>
            </select>
          </label>

          <label class="vault-export-field">
            <span class="vault-export-label">Orientation</span>
            <select
              class="vault-export-select"
              value={options.orientation}
              onchange={(e) =>
                patchOptions({
                  orientation: (e.currentTarget.value ||
                    "portrait") as VaultExportOrientation,
                })}
            >
              <option value="portrait">Portrait</option>
              <option value="landscape">Landscape</option>
            </select>
          </label>

          <label class="vault-export-field">
            <span class="vault-export-label">Margins</span>
            <select
              class="vault-export-select"
              value={options.margins}
              onchange={(e) =>
                patchOptions({
                  margins: (e.currentTarget.value ||
                    "comfortable") as VaultExportMargins,
                })}
            >
              <option value="compact">Compact</option>
              <option value="comfortable">Comfortable</option>
              <option value="wide">Wide</option>
            </select>
          </label>

          <label class="vault-export-check">
            <input
              type="checkbox"
              checked={options.breakBeforeH2}
              onchange={(e) =>
                patchOptions({ breakBeforeH2: e.currentTarget.checked })}
            />
            <span>Break before H2</span>
          </label>

          <label class="vault-export-check">
            <input
              type="checkbox"
              checked={options.keepTogether}
              onchange={(e) =>
                patchOptions({ keepTogether: e.currentTarget.checked })}
            />
            <span>Keep blocks together</span>
          </label>
        </aside>

        <div class="vault-export-preview-body">
          {#if loading}
            <p class="vault-export-preview-status">
              Preparing {format === "pdf" ? "PDF" : "Word"}…
            </p>
          {:else if error}
            <p class="vault-export-preview-error">{error}</p>
          {:else if format === "pdf" && blobUrl}
            <iframe
              class="vault-export-preview-frame"
              title="PDF preview for {title}"
              src={blobUrl}
            ></iframe>
          {:else if format === "docx" && blob}
            <div class="vault-export-docx-ready">
              <p class="vault-export-docx-ready__title">Word document ready</p>
              <p class="vault-export-docx-ready__hint">
                Preview isn’t shown for .docx — settings above are applied. Save to
                download.
              </p>
            </div>
          {/if}
        </div>
      </div>

      <footer class="vault-export-preview-footer">
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={close}
        >
          Close
        </button>
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={!blob || loading || saving}
          onclick={() => void handleSave()}
        >
          {saving
            ? "Saving…"
            : format === "pdf"
              ? "Save PDF…"
              : "Save Word…"}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  :global(.vault-export-preview-sheet) {
    display: flex;
    flex-direction: column;
    width: min(64rem, calc(100vw - 2rem));
    max-width: min(64rem, calc(100vw - 2rem));
    max-height: calc(100vh - 2rem);
    padding: 0.85rem 0.9rem 0.75rem;
  }

  .vault-export-preview-header {
    margin-bottom: 0.65rem;
  }

  .vault-export-preview-layout {
    display: grid;
    grid-template-columns: 12.5rem minmax(0, 1fr);
    gap: 0.75rem;
    flex: 1 1 auto;
    min-height: 0;
  }

  .vault-export-preview-settings {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    padding: 0.65rem 0.7rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-900) / 0.55);
    overflow: auto;
    max-height: 70vh;
  }

  .vault-export-field {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .vault-export-label {
    font-size: 0.65rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .vault-export-select {
    width: 100%;
    border-radius: 0.4rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    background: rgb(var(--color-surface-950) / 0.7);
    color: rgb(var(--color-surface-100));
    font-size: 0.75rem;
    padding: 0.35rem 0.45rem;
  }

  .vault-export-range {
    width: 100%;
  }

  .vault-export-seg {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.25rem;
  }

  .vault-export-seg__btn {
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    border-radius: 0.35rem;
    background: transparent;
    color: rgb(var(--color-surface-300));
    font-size: 0.72rem;
    font-weight: 600;
    padding: 0.35rem 0.4rem;
    cursor: pointer;
  }

  .vault-export-seg__btn--on {
    background: rgb(var(--color-primary-500) / 0.22);
    border-color: rgb(var(--color-primary-400) / 0.45);
    color: rgb(var(--color-surface-50));
  }

  .vault-export-check {
    display: flex;
    align-items: flex-start;
    gap: 0.4rem;
    font-size: 0.72rem;
    color: rgb(var(--color-surface-200));
    cursor: pointer;
  }

  .vault-export-check input {
    margin-top: 0.15rem;
  }

  .vault-export-preview-body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-950) / 0.55);
    overflow: hidden;
  }

  .vault-export-preview-frame {
    width: 100%;
    height: 70vh;
    border: 0;
    background: #ffffff;
  }

  .vault-export-preview-status,
  .vault-export-preview-error,
  .vault-export-docx-ready {
    margin: 0;
    padding: 2.5rem 1.25rem;
    text-align: center;
    font-size: 0.8125rem;
  }

  .vault-export-preview-status {
    color: rgb(var(--color-surface-400));
  }

  .vault-export-preview-error {
    color: rgb(var(--color-error-300));
  }

  .vault-export-docx-ready__title {
    margin: 0 0 0.4rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .vault-export-docx-ready__hint {
    margin: 0;
    color: rgb(var(--color-surface-400));
    line-height: 1.45;
  }

  .vault-export-preview-footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }

  @media (max-width: 720px) {
    .vault-export-preview-layout {
      grid-template-columns: 1fr;
    }

    .vault-export-preview-settings {
      max-height: none;
    }

    .vault-export-preview-frame {
      height: 50vh;
    }
  }
</style>
