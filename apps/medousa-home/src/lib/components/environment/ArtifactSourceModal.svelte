<script lang="ts">
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { Copy, X } from "@lucide/svelte";

  interface Props {
    open: boolean;
    title: string;
    source: string;
    onClose: () => void;
  }

  let { open, title, source, onClose }: Props = $props();
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  async function copySource() {
    try {
      await navigator.clipboard.writeText(source);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied = false;
      }, 2000);
    } catch {
      copied = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!open || event.key !== "Escape") return;
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
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section class="artifact-modal" aria-label={`Source: ${title}`}>
        <header class="artifact-modal-header">
          <div class="min-w-0">
            <h3 class="artifact-modal-title">Source — {title}</h3>
            <p class="artifact-modal-subtitle">HTML stored for this presentation widget</p>
          </div>
          <div class="artifact-modal-actions">
            <button type="button" class="artifact-modal-btn" onclick={() => void copySource()}>
              <Copy size={14} aria-hidden="true" />
              {copied ? "Copied" : "Copy"}
            </button>
            <button type="button" class="artifact-modal-btn" aria-label="Close" onclick={onClose}>
              <X size={14} aria-hidden="true" />
            </button>
          </div>
        </header>
        <pre class="artifact-modal-source"><code>{source}</code></pre>
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
    max-height: min(80vh, 44rem);
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

  .artifact-modal-actions {
    display: flex;
    gap: 0.35rem;
  }

  .artifact-modal-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 55%, transparent);
    border-radius: 0.5rem;
    padding: 0.35rem 0.55rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
    cursor: pointer;
  }

  .artifact-modal-source {
    margin: 0;
    overflow: auto;
    padding: 1rem;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-200));
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
