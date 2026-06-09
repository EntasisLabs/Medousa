<script lang="ts">
  import { ChevronDown, Sparkles } from "@lucide/svelte";

  interface Props {
    reasoning: string;
    /** Keep open while the model is still thinking. */
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
    class="thinking-trace group/thinking mb-2 overflow-hidden rounded-lg border transition-[border-color,background,box-shadow] duration-200 {done
      ? 'border-surface-700/25 bg-surface-900/15'
      : 'border-primary-500/25 bg-gradient-to-r from-primary-500/[0.06] to-surface-900/25 shadow-[inset_0_1px_0_rgba(167,139,250,0.08)]'}"
    open={streaming}
  >
    <summary
      class="flex cursor-pointer list-none items-center gap-2 px-2 py-1 marker:content-none {done
        ? 'py-1'
        : 'px-2.5 py-1.5'}"
    >
      <Sparkles
        class="h-3 w-3 shrink-0 {done
          ? 'text-surface-600'
          : 'text-primary-400/85 animate-pulse'}"
        strokeWidth={2}
        aria-hidden="true"
      />
      <span
        class="min-w-0 flex-1 truncate text-[11px] {done
          ? 'text-surface-500'
          : 'text-primary-200/90'}"
      >
        Thinking
        {#if streaming && liveHook}
          <span class="text-surface-600"> · </span>
          <span class="text-surface-400">{liveHook}</span>
        {/if}
      </span>
      <ChevronDown
        class="h-3 w-3 shrink-0 text-surface-600 transition-transform duration-200 group-open/thinking:rotate-180"
        strokeWidth={2}
        aria-hidden="true"
      />
    </summary>
    <div
      class="border-t px-2.5 pb-2 pt-2 {done
        ? 'border-surface-700/20'
        : 'border-primary-500/10'}"
    >
      <p
        class="whitespace-pre-wrap leading-relaxed text-surface-400 {compact
          ? 'text-xs'
          : 'text-[13px]'} {done ? 'text-surface-500' : 'text-surface-300'}"
      >
        {trimmed}
      </p>
    </div>
  </details>
{/if}
