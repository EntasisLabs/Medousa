<script lang="ts">
  import {
    highlightDiffLine,
    type DiffHighlightSpan,
  } from "$lib/syntax/highlightDiffLine";
  import type { WordPart } from "$lib/diff/wordDiff";

  interface Props {
    content: string;
    languageHint?: string | null;
    prefix?: string;
    parts?: WordPart[] | null;
  }

  let { content, languageHint = null, prefix = "", parts = null }: Props = $props();

  const spans = $derived.by((): DiffHighlightSpan[] => {
    if (parts && parts.length > 0) {
      // When word-diff parts exist, skip full-line Lezer highlight to keep
      // change markers authoritative; still escape via text nodes.
      return parts.map((part) => ({ text: part.text, style: null }));
    }
    return highlightDiffLine(content, languageHint);
  });
</script>

<code class="diff-code">
  {#if prefix}<span class="diff-code-prefix">{prefix}</span>{/if}
  {#if parts && parts.length > 0}
    {#each parts as part, i (`${i}:${part.text.slice(0, 8)}`)}
      <span class:diff-word-changed={part.changed}>{part.text}</span>
    {/each}
  {:else}
    {#each spans as span, i (`${i}:${span.text.slice(0, 8)}`)}
      {#if span.style}
        <span style={span.style}>{span.text}</span>
      {:else}
        {span.text}
      {/if}
    {/each}
  {/if}
</code>

<style>
  .diff-code {
    padding: 0 0.6rem;
    white-space: pre;
  }

  .diff-code-prefix {
    user-select: none;
  }

  .diff-word-changed {
    border-radius: 0.1rem;
    background: rgb(var(--color-warning-500) / 0.28);
    box-decoration-break: clone;
  }
</style>
