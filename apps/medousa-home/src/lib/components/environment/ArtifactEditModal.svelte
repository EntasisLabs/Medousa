<script lang="ts">
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { X } from "@lucide/svelte";

  interface Props {
    open: boolean;
    title: string;
    source: string;
    busy?: boolean;
    error?: string | null;
    onClose: () => void;
    onSave: (html: string) => void | Promise<void>;
  }

  let {
    open,
    title,
    source,
    busy = false,
    error = null,
    onClose,
    onSave,
  }: Props = $props();

  let draft = $state("");

  $effect(() => {
    if (open) draft = source;
  });

  function handleKeydown(event: KeyboardEvent) {
    if (!open || event.key !== "Escape" || busy) return;
    event.preventDefault();
    onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <BodyPortal>
    <div
      class="artifact-modal-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <section class="artifact-modal" aria-label={`Edit: ${title}`}>
        <header class="artifact-modal-header">
          <div class="min-w-0">
            <h3 class="artifact-modal-title">Edit — {title}</h3>
            <p class="artifact-modal-subtitle">Saves a new artifact revision</p>
          </div>
          <button
            type="button"
            class="artifact-modal-btn"
            aria-label="Close"
            disabled={busy}
            onclick={onClose}
          >
            <X size={14} aria-hidden="true" />
          </button>
        </header>
        <textarea
          class="artifact-modal-editor"
          bind:value={draft}
          spellcheck={false}
          disabled={busy}
          aria-label="Artifact HTML source"
        ></textarea>
        {#if error}
          <p class="artifact-modal-error">{error}</p>
        {/if}
        <footer class="artifact-modal-footer">
          <button type="button" class="artifact-modal-btn" disabled={busy} onclick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            class="artifact-modal-btn artifact-modal-btn-primary"
            disabled={busy || !draft.trim()}
            onclick={() => void onSave(draft)}
          >
            {busy ? "Saving…" : "Save revision"}
          </button>
        </footer>
      </section>
    </div>
  </BodyPortal>
{/if}

<style>
  .artifact-modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    display: grid;
    place-items: center;
    padding: 1rem;
    background: rgb(0 0 0 / 0.45);
  }

  .artifact-modal {
    display: flex;
    width: min(52rem, 100%);
    max-height: min(84vh, 48rem);
    flex-direction: column;
    border-radius: 0.875rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 50%, transparent);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 24px 64px rgb(0 0 0 / 0.28);
  }

  .artifact-modal-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.875rem 1rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-600) 40%, transparent);
  }

  .artifact-modal-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .artifact-modal-subtitle {
    margin: 0.2rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-500));
  }

  .artifact-modal-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 55%, transparent);
    border-radius: 0.5rem;
    padding: 0.35rem 0.65rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
    cursor: pointer;
  }

  .artifact-modal-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .artifact-modal-btn-primary {
    color: rgb(var(--color-surface-50));
    border-color: color-mix(in srgb, var(--color-primary-400) 55%, transparent);
    background: rgb(var(--color-primary-600));
  }

  .artifact-modal-editor {
    min-height: 18rem;
    flex: 1 1 auto;
    resize: vertical;
    margin: 0;
    padding: 1rem;
    border: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-100));
    background: rgb(var(--color-surface-950));
  }

  .artifact-modal-error {
    margin: 0;
    padding: 0 1rem 0.5rem;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }

  .artifact-modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    padding: 0.75rem 1rem 1rem;
  }
</style>
