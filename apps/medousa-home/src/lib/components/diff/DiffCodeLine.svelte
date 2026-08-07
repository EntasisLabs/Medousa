<script lang="ts">
  import {
    highlightDiffLine,
    type DiffHighlightSpan,
  } from "$lib/syntax/highlightDiffLine";
  import type { WordPart } from "$lib/diff/wordDiff";

  type MergedSpan = {
    text: string;
    style: string | null;
    changed: boolean;
  };

  interface Props {
    content: string;
    languageHint?: string | null;
    /** Word-level change parts for this side of a replacement. */
    parts?: WordPart[] | null;
    /** Side-aware word emphasis tone. */
    tone?: "add" | "del" | null;
    wrap?: boolean;
  }

  let {
    content,
    languageHint = null,
    parts = null,
    tone = null,
    wrap = false,
  }: Props = $props();

  function mergeHighlightAndWordParts(
    line: string,
    syntax: DiffHighlightSpan[],
    wordParts: WordPart[],
  ): MergedSpan[] {
    const joined = wordParts.map((part) => part.text).join("");
    if (joined !== line) {
      return wordParts.map((part) => ({
        text: part.text,
        style: null,
        changed: part.changed,
      }));
    }

    const partRanges: Array<{ start: number; end: number; changed: boolean }> = [];
    let offset = 0;
    for (const part of wordParts) {
      partRanges.push({
        start: offset,
        end: offset + part.text.length,
        changed: part.changed,
      });
      offset += part.text.length;
    }

    const synRanges: Array<{ start: number; end: number; style: string | null }> = [];
    offset = 0;
    for (const span of syntax) {
      synRanges.push({
        start: offset,
        end: offset + span.text.length,
        style: span.style,
      });
      offset += span.text.length;
    }
    if (offset !== line.length) {
      return wordParts.map((part) => ({
        text: part.text,
        style: null,
        changed: part.changed,
      }));
    }

    const bounds = new Set<number>([0, line.length]);
    for (const range of partRanges) {
      bounds.add(range.start);
      bounds.add(range.end);
    }
    for (const range of synRanges) {
      bounds.add(range.start);
      bounds.add(range.end);
    }
    const sorted = [...bounds].sort((a, b) => a - b);
    const merged: MergedSpan[] = [];
    for (let i = 0; i < sorted.length - 1; i += 1) {
      const start = sorted[i]!;
      const end = sorted[i + 1]!;
      if (start === end) continue;
      const part = partRanges.find((range) => range.start <= start && end <= range.end);
      const syn = synRanges.find((range) => range.start <= start && end <= range.end);
      merged.push({
        text: line.slice(start, end),
        style: syn?.style ?? null,
        changed: part?.changed ?? false,
      });
    }
    return merged;
  }

  const spans = $derived.by((): MergedSpan[] => {
    const syntax = highlightDiffLine(content, languageHint);
    if (parts && parts.length > 0) {
      return mergeHighlightAndWordParts(content, syntax, parts);
    }
    return syntax.map((span) => ({
      text: span.text,
      style: span.style,
      changed: false,
    }));
  });
</script>

<code class="diff-code" class:diff-code--wrap={wrap}>
  {#each spans as span, i (`${i}:${span.text.slice(0, 8)}`)}
    {#if span.changed && tone}
      <span
        class="diff-word-changed"
        class:diff-word-changed--add={tone === "add"}
        class:diff-word-changed--del={tone === "del"}
        style={span.style}
      >{span.text}</span>
    {:else if span.style}
      <span style={span.style}>{span.text}</span>
    {:else}
      {span.text}
    {/if}
  {/each}
</code>

<style>
  .diff-code {
    padding: 0 0.6rem;
    white-space: pre;
    min-width: 0;
  }

  .diff-code--wrap {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .diff-word-changed {
    border-radius: 0.1rem;
    box-decoration-break: clone;
  }

  .diff-word-changed--add {
    background: rgb(var(--syn-addition-bg) / 0.3);
  }

  .diff-word-changed--del {
    background: rgb(var(--syn-deletion-bg) / 0.14);
  }
</style>
