<script lang="ts">
  /** `callout` molecule — toned aside (note / warn / error / success). */
  import { ChevronDown } from "@lucide/svelte";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { renderInlineMarkdown } from "$lib/markdown";

  type Tone = "note" | "warn" | "error" | "success";
  const TONES: Tone[] = ["note", "warn", "error", "success"];

  let { node }: ArchetypeProps = $props();

  const tone = $derived<Tone>(TONES.includes(node.props.tone as Tone) ? (node.props.tone as Tone) : "note");
  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const body = $derived(typeof node.props.body === "string" ? node.props.body : "");
  const detail = $derived(
    typeof node.props.detail === "string" ? node.props.detail.trim() : "",
  );
</script>

<div
  class="liquid-callout"
  data-tone={tone}
  role={tone === "error" || tone === "warn" ? "alert" : undefined}
>
  {#if title}<p class="liquid-callout-title">{@html renderInlineMarkdown(title)}</p>{/if}
  {#if body}<p class="liquid-callout-body">{@html renderInlineMarkdown(body)}</p>{/if}
  {#if detail}
    <details class="liquid-callout-details group/err-detail">
      <summary
        class="mt-1.5 flex cursor-pointer list-none items-center gap-1 text-[10px] font-medium text-inherit/80 marker:content-none hover:opacity-100 opacity-80"
      >
        <span>View details</span>
        <ChevronDown
          class="h-3 w-3 shrink-0 transition-transform duration-200 group-open/err-detail:rotate-180"
          strokeWidth={2}
          aria-hidden="true"
        />
      </summary>
      <pre
        class="liquid-callout-detail-body mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-[10px] leading-relaxed opacity-90"
      >{detail}</pre>
    </details>
  {/if}
</div>

<style>
  .liquid-callout {
    padding: 0.5rem 0.65rem;
    border-radius: 0.5rem;
    border: 1px solid transparent;
    font-size: 0.75rem;
  }

  .liquid-callout-title {
    margin: 0 0 0.15rem;
    font-weight: 600;
  }

  .liquid-callout-body {
    margin: 0;
    line-height: 1.5;
  }

  .liquid-callout-details summary::-webkit-details-marker {
    display: none;
  }

  .liquid-callout-detail-body {
    margin: 0;
    padding: 0.4rem 0.5rem;
    border-radius: 0.35rem;
    background: color-mix(in srgb, currentColor 8%, transparent);
  }

  .liquid-callout[data-tone="note"] {
    color: rgb(var(--color-surface-200));
    background: color-mix(in srgb, var(--color-surface-700) 45%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 40%, transparent);
  }

  .liquid-callout[data-tone="warn"] {
    color: rgb(var(--color-warning-200));
    background: color-mix(in srgb, var(--color-warning-500) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-warning-500) 45%, transparent);
  }

  .liquid-callout[data-tone="error"] {
    color: rgb(var(--color-error-300));
    background: color-mix(in srgb, var(--color-error-500) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-error-500) 45%, transparent);
  }

  .liquid-callout[data-tone="success"] {
    color: rgb(var(--color-success-300));
    background: color-mix(in srgb, var(--color-success-500) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-success-500) 45%, transparent);
  }
</style>
