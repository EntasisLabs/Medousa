<script lang="ts">
  /**
   * Whole-note slides deck surface (`kind: slides` / medousa-deck).
   * Resting: Preview organism; Write: fence-body editor with serialize round-trip.
   */
  import { onDestroy } from "svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import {
    parseSlidesDeck,
    replaceSlidesDeck,
    serializeSlidesDeckBody,
    slidesDeckFromContent,
    type SlidesDeck,
  } from "$lib/utils/markdownSlides";
  import { vault } from "$lib/stores/vault.svelte";

  interface Props {
    content: string;
    disabled?: boolean;
    onchange: (nextContent: string) => void;
  }

  let { content, disabled = false, onchange }: Props = $props();

  let deck = $state<SlidesDeck>(slidesDeckFromContent(content));
  let syncedContent = $state(content);
  let editing = $state(false);
  let draftBody = $state("");
  let draftTitle = $state("");
  let emitTimer: ReturnType<typeof setTimeout> | null = null;

  const fencePreview = $derived(
    "```slides\n" + serializeSlidesDeckBody(deck).trimEnd() + "\n```\n",
  );

  function emit(next: SlidesDeck) {
    deck = next;
    const markdown = replaceSlidesDeck(content, next);
    syncedContent = markdown;
    if (emitTimer) clearTimeout(emitTimer);
    emitTimer = setTimeout(() => {
      emitTimer = null;
      onchange(markdown);
    }, 120);
  }

  $effect(() => {
    if (content === syncedContent) return;
    deck = slidesDeckFromContent(content);
    syncedContent = content;
    if (editing) {
      draftTitle = deck.title;
      draftBody = serializeSlidesDeckBody(deck).trim();
    }
  });

  onDestroy(() => {
    if (emitTimer) clearTimeout(emitTimer);
  });

  function setColumns(columns: "1" | "2" | "3") {
    if (disabled || deck.columns === columns) return;
    emit({ ...deck, columns });
  }

  function beginWrite() {
    if (disabled) return;
    editing = true;
    draftTitle = deck.title;
    draftBody = serializeSlidesDeckBody(deck).trim();
  }

  function finishWrite() {
    const next = parseSlidesDeck(draftBody);
    if (next) {
      emit({
        ...next,
        title: draftTitle.trim() || next.title,
        columns: next.columns || deck.columns,
      });
    } else {
      emit({ ...deck, title: draftTitle.trim() });
    }
    editing = false;
  }
</script>

<div class="slides-deck-editor" class:slides-deck-editor--disabled={disabled}>
  <div class="slides-deck-chrome">
    <div class="slides-deck-cols" role="group" aria-label="Figure columns">
      {#each ["1", "2", "3"] as col (col)}
        <button
          type="button"
          class="slides-deck-col"
          class:slides-deck-col--active={deck.columns === col}
          disabled={disabled}
          aria-pressed={deck.columns === col}
          onclick={() => setColumns(col as "1" | "2" | "3")}
        >
          {col} col
        </button>
      {/each}
    </div>
    <button
      type="button"
      class="slides-deck-write"
      disabled={disabled}
      onclick={() => (editing ? finishWrite() : beginWrite())}
    >
      {editing ? "Done" : "Write"}
    </button>
  </div>

  {#if editing}
    <div class="slides-deck-write-panel">
      <input
        class="slides-deck-title"
        type="text"
        placeholder="Deck title"
        bind:value={draftTitle}
        {disabled}
      />
      <textarea
        class="slides-deck-body"
        rows="18"
        placeholder={"---\nlabel: Title\nlayout: hero\n\n# Slide…"}
        bind:value={draftBody}
        {disabled}
      ></textarea>
    </div>
  {:else}
    <div class="slides-deck-stage markdown-content">
      <MarkdownContent
        content={fencePreview}
        titleByPath={vault.labelByPathMap}
      />
    </div>
  {/if}
</div>

<style>
  .slides-deck-editor {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    padding: 0.75rem 1rem 1.25rem;
    gap: 0.75rem;
    overflow: auto;
  }

  .slides-deck-editor--disabled {
    opacity: 0.65;
    pointer-events: none;
  }

  .slides-deck-chrome {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .slides-deck-cols {
    display: flex;
    gap: 0.35rem;
  }

  .slides-deck-col,
  .slides-deck-write {
    appearance: none;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 32%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 40%, transparent);
    color: rgb(var(--color-surface-200));
    border-radius: 0.5rem;
    padding: 0.28rem 0.65rem;
    font-size: 0.75rem;
    font-weight: 550;
    cursor: pointer;
  }

  .slides-deck-col--active {
    color: rgb(var(--color-surface-50));
    border-color: color-mix(in srgb, var(--color-surface-400) 50%, transparent);
  }

  .slides-deck-write-panel {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    min-height: 0;
    flex: 1;
  }

  .slides-deck-title,
  .slides-deck-body {
    width: 100%;
    border-radius: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 55%, transparent);
    color: rgb(var(--color-surface-100));
    padding: 0.55rem 0.7rem;
    font: inherit;
  }

  .slides-deck-body {
    flex: 1;
    min-height: 16rem;
    resize: vertical;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.8125rem;
    line-height: 1.45;
  }

  .slides-deck-stage {
    min-width: 0;
  }
</style>
