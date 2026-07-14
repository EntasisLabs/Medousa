<script lang="ts">
  /**
   * Full-screen Liquid card detail sheet (Monogram-style expand).
   * Chip tap prefills the composer via onChipSelect — does not auto-send.
   */
  import { X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { renderInlineMarkdown, renderMarkdown } from "$lib/markdown";
  import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";

  interface Props {
    open: boolean;
    detail: CardDetailPayload | null;
    onClose: () => void;
    onChipSelect?: (label: string) => void;
  }

  let { open, detail, onClose, onChipSelect }: Props = $props();

  function handleClose() {
    haptic("light");
    onClose();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!open || event.key !== "Escape") return;
    event.preventDefault();
    handleClose();
  }

  function selectChip(label: string) {
    haptic("light");
    onChipSelect?.(label);
  }

  $effect(() => {
    if (!open) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  });

  $effect(() => {
    if (!open) return;
    return registerMobileBackHandler(() => {
      handleClose();
      return true;
    });
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open && detail}
  <BodyPortal>
    <div
      class="liquid-card-detail-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={detail.title}
      tabindex="-1"
      onclick={(event) => {
        if (event.target === event.currentTarget) handleClose();
      }}
      onkeydown={handleKeydown}
    >
      <div class="liquid-card-detail-sheet">
        <header class="liquid-card-detail-chrome">
          <button
            type="button"
            class="liquid-card-detail-close"
            aria-label="Close"
            onclick={handleClose}
          >
            <X size={18} strokeWidth={2.25} />
          </button>
        </header>

        <div class="liquid-card-detail-body">
          {#if detail.image}
            <img class="liquid-card-detail-hero-img" src={detail.image} alt="" />
          {:else if detail.emoji}
            <span class="liquid-card-detail-hero-emoji" aria-hidden="true">{detail.emoji}</span>
          {/if}

          <h2 class="liquid-card-detail-title">{@html renderInlineMarkdown(detail.title)}</h2>

          {#if detail.meta}
            <p class="liquid-card-detail-meta">{@html renderInlineMarkdown(detail.meta)}</p>
          {:else if detail.subtitle}
            <p class="liquid-card-detail-meta">{@html renderInlineMarkdown(detail.subtitle)}</p>
          {/if}

          {#if detail.summary}
            <div class="liquid-card-detail-summary markdown-body">
              {@html renderMarkdown(detail.summary)}
            </div>
          {/if}

          {#if detail.chips?.length}
            <div class="liquid-card-detail-chips">
              {#each detail.chips as chip, i (i)}
                <button
                  type="button"
                  class="liquid-card-detail-chip"
                  onclick={() => selectChip(chip)}
                >
                  {chip}
                </button>
              {/each}
            </div>
          {/if}

          {#if detail.points?.length}
            <ul class="liquid-card-detail-points">
              {#each detail.points as point, i (i)}
                <li class="liquid-card-detail-point">
                  {#if point.emoji}
                    <span class="liquid-card-detail-point-emoji" aria-hidden="true"
                      >{point.emoji}</span
                    >
                  {/if}
                  <span class="liquid-card-detail-point-text">
                    <span class="liquid-card-detail-point-label">{point.label}</span>
                    <span class="liquid-card-detail-point-body">{point.body}</span>
                  </span>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .liquid-card-detail-backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--color-surface-950) 72%, transparent);
    padding: max(0.5rem, env(safe-area-inset-top, 0px))
      max(0.5rem, env(safe-area-inset-right, 0px))
      max(0.5rem, env(safe-area-inset-bottom, 0px))
      max(0.5rem, env(safe-area-inset-left, 0px));
    animation: liquid-card-detail-backdrop-in 220ms ease-out;
  }

  .liquid-card-detail-sheet {
    display: flex;
    flex-direction: column;
    width: min(32rem, 100%);
    max-height: 100%;
    height: fit-content;
    margin: 0 auto;
    border-radius: 1.25rem;
    background: rgb(var(--color-surface-50));
    color: rgb(var(--color-surface-900));
    overflow: hidden;
    box-shadow: 0 18px 48px color-mix(in srgb, var(--color-surface-950) 35%, transparent);
    animation: liquid-card-detail-sheet-in 320ms cubic-bezier(0.22, 1, 0.36, 1) both;
    will-change: transform, opacity;
  }

  :global(.dark) .liquid-card-detail-sheet,
  :global([data-mode="dark"]) .liquid-card-detail-sheet {
    background: rgb(var(--color-surface-900));
    color: rgb(var(--color-surface-50));
  }

  .liquid-card-detail-chrome {
    display: flex;
    align-items: center;
    padding: 0.75rem 0.85rem 0.25rem;
  }

  .liquid-card-detail-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.25rem;
    height: 2.25rem;
    margin: 0;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-surface-500) 18%, transparent);
    color: inherit;
    cursor: pointer;
  }

  .liquid-card-detail-close:hover {
    background: color-mix(in srgb, var(--color-surface-500) 28%, transparent);
  }

  .liquid-card-detail-body {
    flex: 0 1 auto;
    min-height: 0;
    overflow: auto;
    padding: 0.5rem 1.35rem 1.75rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 0.65rem;
  }

  .liquid-card-detail-hero-emoji {
    font-size: 3.25rem;
    line-height: 1;
    margin: 0.35rem 0 0.15rem;
  }

  .liquid-card-detail-hero-img {
    width: 4.5rem;
    height: 4.5rem;
    border-radius: 1rem;
    object-fit: cover;
    margin: 0.35rem 0 0.15rem;
  }

  .liquid-card-detail-title {
    margin: 0;
    font-size: 1.85rem;
    font-weight: 700;
    letter-spacing: -0.03em;
    line-height: 1.15;
  }

  .liquid-card-detail-meta {
    margin: 0;
    font-size: 0.78rem;
    color: color-mix(in srgb, currentColor 55%, transparent);
  }

  .liquid-card-detail-summary {
    margin: 0.35rem 0 0;
    width: 100%;
    max-width: 28rem;
    font-size: 0.95rem;
    line-height: 1.55;
    text-align: left;
    color: color-mix(in srgb, currentColor 82%, transparent);
  }

  .liquid-card-detail-summary :global(p) {
    margin: 0 0 0.75em;
  }

  .liquid-card-detail-summary :global(p:last-child) {
    margin-bottom: 0;
  }

  .liquid-card-detail-summary :global(ul),
  .liquid-card-detail-summary :global(ol) {
    margin: 0 0 0.75em;
    padding-left: 1.25em;
  }

  .liquid-card-detail-chips {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.45rem;
    margin-top: 0.35rem;
  }

  .liquid-card-detail-chip {
    margin: 0;
    padding: 0.4rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-500) 10%, transparent);
    color: inherit;
    font-size: 0.78rem;
    font-weight: 550;
    cursor: pointer;
  }

  .liquid-card-detail-chip:hover {
    background: color-mix(in srgb, var(--color-primary-500) 14%, transparent);
    border-color: color-mix(in srgb, var(--color-primary-500) 35%, transparent);
  }

  .liquid-card-detail-points {
    list-style: none;
    margin: 0.85rem 0 0;
    padding: 0;
    width: 100%;
    max-width: 26rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    text-align: left;
  }

  .liquid-card-detail-point {
    display: flex;
    align-items: flex-start;
    gap: 0.7rem;
  }

  .liquid-card-detail-point-emoji {
    font-size: 1.35rem;
    line-height: 1.2;
    flex-shrink: 0;
  }

  .liquid-card-detail-point-text {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }

  .liquid-card-detail-point-label {
    font-size: 0.95rem;
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .liquid-card-detail-point-body {
    font-size: 0.82rem;
    line-height: 1.45;
    color: color-mix(in srgb, currentColor 58%, transparent);
  }

  @keyframes liquid-card-detail-backdrop-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes liquid-card-detail-sheet-in {
    from {
      opacity: 0;
      transform: translateY(16px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-card-detail-backdrop,
    .liquid-card-detail-sheet {
      animation: none;
    }
  }
</style>
