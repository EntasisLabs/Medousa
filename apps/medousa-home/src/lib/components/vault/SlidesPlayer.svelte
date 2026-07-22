<script lang="ts">
  /**
   * Fullscreen slideshow player — ←/→/Esc, progress, speaker notes, motion v1.
   */
  import { onDestroy, onMount } from "svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import {
    serializeSlidesDeckBody,
    type SlidesDeck,
  } from "$lib/utils/markdownSlides";
  import { vault } from "$lib/stores/vault.svelte";

  interface Props {
    deck: SlidesDeck;
    open?: boolean;
    onclose?: () => void;
  }

  let { deck, open = true, onclose }: Props = $props();

  let index = $state(0);
  let rootEl: HTMLDivElement | null = $state(null);
  let motionKey = $state(0);

  const slide = $derived(deck.slides[index] ?? null);
  const progress = $derived(
    deck.slides.length <= 1 ? 1 : index / (deck.slides.length - 1),
  );

  const fencePreview = $derived.by(() => {
    if (!slide) return "";
    const single: SlidesDeck = {
      title: deck.title,
      theme: deck.theme,
      columns: deck.columns,
      slides: [slide],
    };
    return "```slides\n" + serializeSlidesDeckBody(single).trimEnd() + "\n```\n";
  });

  function close() {
    onclose?.();
  }

  function go(delta: number) {
    if (deck.slides.length === 0) return;
    const next = Math.max(0, Math.min(deck.slides.length - 1, index + delta));
    if (next === index) return;
    index = next;
    motionKey += 1;
  }

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      return;
    }
    if (e.key === "ArrowRight" || e.key === " " || e.key === "PageDown") {
      e.preventDefault();
      go(1);
      return;
    }
    if (e.key === "ArrowLeft" || e.key === "PageUp") {
      e.preventDefault();
      go(-1);
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    void rootEl?.requestFullscreen?.().catch(() => {
      /* fullscreen may be blocked — still usable as overlay */
    });
  });

  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    if (document.fullscreenElement) {
      void document.exitFullscreen?.().catch(() => {});
    }
  });

  $effect(() => {
    if (!open) return;
    index = 0;
    motionKey = 0;
  });
</script>

{#if open && slide}
  <div
    class="slides-player"
    bind:this={rootEl}
    role="dialog"
    aria-modal="true"
    aria-label={deck.title || "Slideshow"}
  >
    <div class="slides-player-top">
      <span class="slides-player-title">{deck.title || "Slideshow"}</span>
      <span class="slides-player-count"
        >{index + 1} / {deck.slides.length}</span
      >
      <button type="button" class="slides-player-close" onclick={close}>
        Esc
      </button>
    </div>

    <div
      class="slides-player-stage"
      class:slides-player-stage--fade={slide.motion === "fade"}
      class:slides-player-stage--fade-up={slide.motion === "fade-up"}
      data-motion-key={motionKey}
    >
      <div class="slides-player-frame markdown-content">
        <MarkdownContent
          content={fencePreview}
          titleByPath={vault.labelByPathMap}
          liquidContext={{
            titleByPath: vault.labelByPathMap,
            localImagePath: vault.selectedPath,
          }}
        />
      </div>
    </div>

    <div class="slides-player-progress" aria-hidden="true">
      <div class="slides-player-progress-fill" style:width={`${progress * 100}%`}
      ></div>
    </div>

    {#if slide.notes?.trim()}
      <aside class="slides-player-notes">
        <p class="slides-player-notes-label">Notes</p>
        <p class="slides-player-notes-body">{slide.notes}</p>
      </aside>
    {/if}

    <div class="slides-player-nav">
      <button type="button" onclick={() => go(-1)} disabled={index <= 0}>←</button>
      <button
        type="button"
        onclick={() => go(1)}
        disabled={index >= deck.slides.length - 1}>→</button
      >
    </div>
  </div>
{/if}

<style>
  .slides-player {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    grid-template-rows: auto 1fr auto auto auto;
    gap: 0.5rem;
    padding: 0.75rem 1rem 1rem;
    background: #0a0a0b;
    color: #f4f4f5;
  }

  .slides-player-top {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .slides-player-title {
    font-weight: 650;
    letter-spacing: -0.02em;
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .slides-player-count {
    font-size: 0.75rem;
    opacity: 0.65;
  }

  .slides-player-close,
  .slides-player-nav button {
    appearance: none;
    border: 1px solid color-mix(in srgb, #fff 18%, transparent);
    background: color-mix(in srgb, #fff 6%, transparent);
    color: inherit;
    border-radius: 0.5rem;
    padding: 0.28rem 0.7rem;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .slides-player-nav button:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .slides-player-stage {
    min-height: 0;
    display: grid;
    place-items: center;
  }

  .slides-player-stage--fade {
    animation: slides-fade 280ms ease-out;
  }

  .slides-player-stage--fade-up {
    animation: slides-fade-up 320ms ease-out;
  }

  @keyframes slides-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes slides-fade-up {
    from {
      opacity: 0;
      transform: translateY(12px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .slides-player-frame {
    width: min(100%, 1120px);
    min-width: 0;
  }

  .slides-player-progress {
    height: 3px;
    border-radius: 999px;
    background: color-mix(in srgb, #fff 12%, transparent);
    overflow: hidden;
  }

  .slides-player-progress-fill {
    height: 100%;
    background: color-mix(in srgb, #fff 55%, transparent);
    transition: width 160ms ease;
  }

  .slides-player-notes {
    border-top: 1px solid color-mix(in srgb, #fff 12%, transparent);
    padding-top: 0.5rem;
    max-height: 5.5rem;
    overflow: auto;
  }

  .slides-player-notes-label {
    margin: 0 0 0.2rem;
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.55;
  }

  .slides-player-notes-body {
    margin: 0;
    font-size: 0.85rem;
    line-height: 1.4;
    opacity: 0.9;
  }

  .slides-player-nav {
    display: flex;
    justify-content: center;
    gap: 0.5rem;
  }
</style>
