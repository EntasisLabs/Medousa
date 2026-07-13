<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { X } from "@lucide/svelte";
  import {
    renderVaultNotePdfBlob,
    saveVaultNotePdfBlob,
    vaultPdfFilename,
  } from "$lib/utils/vaultPdfExport";

  interface Props {
    open: boolean;
    title: string;
    content: string;
    labelByPath: Map<string, string>;
    onClose: () => void;
    /** True while the PDF blob is being rendered (for overflow menu label). */
    onPreparingChange?: (preparing: boolean) => void;
  }

  let {
    open,
    title,
    content,
    labelByPath,
    onClose,
    onPreparingChange,
  }: Props = $props();

  let blob = $state<Blob | null>(null);
  let blobUrl = $state<string | null>(null);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let renderGen = 0;

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
    untrack(() => {
      revokeBlobUrl();
      error = null;
      saving = false;
      setPreparing(false);
    });
    onClose();
  }

  $effect(() => {
    if (!open) {
      untrack(() => {
        renderGen += 1;
        revokeBlobUrl();
        error = null;
        setPreparing(false);
      });
      return;
    }

    const noteTitle = title;
    const noteContent = content;
    const labels = labelByPath;
    const gen = ++renderGen;

    untrack(() => {
      revokeBlobUrl();
      error = null;
      setPreparing(true);
    });

    void (async () => {
      try {
        const next = await renderVaultNotePdfBlob({
          title: noteTitle,
          content: noteContent,
          labelByPath: labels,
        });
        if (gen !== renderGen) return;
        blob = next;
        blobUrl = URL.createObjectURL(next);
      } catch (err) {
        if (gen !== renderGen) return;
        error = err instanceof Error ? err.message : String(err);
      } finally {
        if (gen === renderGen) setPreparing(false);
      }
    })();
  });

  onDestroy(() => {
    renderGen += 1;
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
      const saved = await saveVaultNotePdfBlob(blob, vaultPdfFilename(title));
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
    aria-labelledby="vault-pdf-preview-title"
    tabindex="-1"
    onkeydown={onSheetKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) close();
    }}
  >
    <div class="vault-interact-sheet vault-pdf-preview-sheet">
      <header class="vault-interact-header vault-pdf-preview-header">
        <div class="min-w-0">
          <p class="vault-interact-kicker">PDF preview</p>
          <h3 id="vault-pdf-preview-title" class="truncate text-sm font-semibold text-surface-50">
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

      <div class="vault-pdf-preview-body">
        {#if loading}
          <p class="vault-pdf-preview-status">Preparing PDF…</p>
        {:else if error}
          <p class="vault-pdf-preview-error">{error}</p>
        {:else if blobUrl}
          <iframe
            class="vault-pdf-preview-frame"
            title="PDF preview for {title}"
            src={blobUrl}
          ></iframe>
        {/if}
      </div>

      <footer class="vault-pdf-preview-footer">
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
          {saving ? "Saving…" : "Save PDF…"}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  :global(.vault-pdf-preview-sheet) {
    display: flex;
    flex-direction: column;
    width: min(52rem, calc(100vw - 2rem));
    max-width: min(52rem, calc(100vw - 2rem));
    max-height: calc(100vh - 2rem);
    padding: 0.85rem 0.9rem 0.75rem;
  }

  .vault-pdf-preview-header {
    margin-bottom: 0.65rem;
  }

  .vault-pdf-preview-body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-950) / 0.55);
    overflow: hidden;
  }

  .vault-pdf-preview-frame {
    width: 100%;
    height: 70vh;
    border: 0;
    background: #ffffff;
  }

  .vault-pdf-preview-status,
  .vault-pdf-preview-error {
    margin: 0;
    padding: 2.5rem 1.25rem;
    text-align: center;
    font-size: 0.8125rem;
  }

  .vault-pdf-preview-status {
    color: rgb(var(--color-surface-400));
  }

  .vault-pdf-preview-error {
    color: rgb(var(--color-error-300));
  }

  .vault-pdf-preview-footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }
</style>
