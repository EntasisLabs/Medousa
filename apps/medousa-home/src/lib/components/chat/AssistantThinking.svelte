<script lang="ts">
  import AudioLinesMark from "$lib/components/ui/AudioLinesMark.svelte";
  import { ChevronDown } from "@lucide/svelte";

  interface Props {
    reasoning: string;
    /** Live cue while the model is still thinking (does not force expand). */
    streaming?: boolean;
    compact?: boolean;
  }

  let { reasoning, streaming = false, compact = false }: Props = $props();

  const trimmed = $derived(reasoning.trim());
  const done = $derived(!streaming);

  /** One short hook for the live summary only — never a paragraph when collapsed. */
  function thinkingHook(text: string): string {
    const first =
      text
        .split("\n")
        .map((line) => line.trim())
        .find(Boolean) ?? "";
    const sentence = first.match(/^[^.!?]+[.!?]?/)?.[0]?.trim() ?? first;
    if (sentence.length <= 44) return sentence;
    return `${sentence.slice(0, 43)}…`;
  }

  const liveHook = $derived(thinkingHook(trimmed));
</script>

{#if trimmed}
  <details
    class="thinking-trace group/thinking overflow-hidden transition-[color,opacity] duration-200"
    class:thinking-live={!done}
    class:thinking-done={done}
  >
    <summary
      class="flex cursor-pointer list-none items-center gap-1.5 marker:content-none {done
        ? 'py-0.5'
        : 'px-0 py-1'}"
    >
      <span
        class="thinking-audio-mark shrink-0 {done
          ? 'text-surface-700'
          : 'text-content-tertiary'}"
      >
        <AudioLinesMark hot={!done} lit={!done} size={12} />
      </span>
      <span
        class="min-w-0 flex-1 truncate text-[10px] {done
          ? 'font-normal text-content-faint'
          : 'text-content-tertiary'}"
      >
        Thinking
        {#if streaming && liveHook}
          <span class="text-content-faint"> · </span>
          <span class="text-content-quiet">{liveHook}</span>
        {/if}
      </span>
      <ChevronDown
        class="h-3 w-3 shrink-0 text-surface-700 transition-transform duration-200 group-open/thinking:rotate-180"
        strokeWidth={2}
        aria-hidden="true"
      />
    </summary>
    <div class="pt-1 {done ? 'pl-4' : 'pl-0.5'}">
      <p
        class="whitespace-pre-wrap leading-relaxed {compact
          ? 'text-[11px]'
          : 'text-[11px]'} {done ? 'text-content-faint' : 'text-content-quiet'}"
      >
        {trimmed}
      </p>
    </div>
  </details>
{/if}

<style>
  /* Live: soft tint, no heavy card chrome — feels alive without a box. */
  .thinking-live {
    margin-bottom: 0.65rem;
    border-radius: 0.5rem;
    padding-inline: 0.15rem;
  }

  /* Done: footnote energy — quiet, more air before the answer body. */
  .thinking-done {
    margin-bottom: 0.85rem;
    opacity: 0.55;
  }

  .thinking-done:hover,
  .thinking-done[open] {
    opacity: 0.85;
  }

  .thinking-audio-mark {
    display: inline-flex;
    line-height: 0;
  }
</style>
