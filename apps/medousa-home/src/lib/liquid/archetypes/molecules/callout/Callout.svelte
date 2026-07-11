<script lang="ts">
  /** `callout` molecule — toned aside (note / warn / error / success). */
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { renderInlineMarkdown } from "$lib/markdown";

  type Tone = "note" | "warn" | "error" | "success";
  const TONES: Tone[] = ["note", "warn", "error", "success"];

  let { node }: ArchetypeProps = $props();

  const tone = $derived<Tone>(TONES.includes(node.props.tone as Tone) ? (node.props.tone as Tone) : "note");
  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const body = $derived(typeof node.props.body === "string" ? node.props.body : "");
</script>

<div
  class="liquid-callout"
  data-tone={tone}
  role={tone === "error" || tone === "warn" ? "alert" : undefined}
>
  {#if title}<p class="liquid-callout-title">{@html renderInlineMarkdown(title)}</p>{/if}
  {#if body}<p class="liquid-callout-body">{@html renderInlineMarkdown(body)}</p>{/if}
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
    color: rgb(var(--color-success-200));
    background: color-mix(in srgb, var(--color-success-500) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-success-500) 45%, transparent);
  }
</style>
