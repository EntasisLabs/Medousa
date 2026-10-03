<script lang="ts">
  /**
   * `code` molecule — compact snippet: language label, copy, optional diff tint.
   * Paste-first from ```code markdown (not a mistaken prose ```code fence).
   */
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { highlightElement } from "$lib/syntax/highlightCode";
  import "$lib/styles/markdown-content.postcss";
  import { onDestroy } from "svelte";
  import { haptic } from "$lib/haptics";
  import {
    canWrapCode,
    codeCopyContent,
    codeLanguageLabel,
    codeLineCount,
    copyCodeText,
    isLongCode,
    type CodeCopyState,
  } from "$lib/markdown/codeBlockPresentation";

  let { node }: ArchetypeProps = $props();

  const source = $derived(typeof node.props.source === "string" ? node.props.source : "");
  const lang = $derived(
    typeof node.props.lang === "string" ? node.props.lang.trim().toLowerCase() : "",
  );
  const title = $derived(typeof node.props.title === "string" ? node.props.title.trim() : "");
  const isDiff = $derived(
    node.props.diff === true || lang === "diff",
  );
  const showCopy = $derived(node.props.copy !== false);

  const lineCount = $derived(codeLineCount(source));
  const collapsible = $derived(isLongCode(source));
  let expanded = $state(false);
  let wrapped = $state(false);
  let copyState = $state<CodeCopyState>("idle");
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  let codeEl = $state<HTMLElement | null>(null);

  $effect(() => {
    const el = codeEl;
    const src = source;
    const language = lang;
    if (!el || isDiff) return;
    delete el.dataset.hljs;
    el.textContent = src;
    el.className = language ? `markdown-code syn-code language-${language}` : "markdown-code syn-code";
    void highlightElement(el, language || "plaintext").catch(() => {});
  });

  $effect(() => { source; expanded = false; });
  onDestroy(() => { if (copyTimer) clearTimeout(copyTimer); });

  async function copySource() {
    const ok = await copyCodeText(source);
    copyState = ok ? "copied" : "failed";
    if (ok) haptic("light");
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => {
      copyState = "idle";
      copyTimer = null;
    }, 1500);
  }

  interface DiffLine {
    kind: "add" | "del" | "ctx";
    text: string;
  }

  const lines = $derived.by((): DiffLine[] | null => {
    if (!isDiff || !source) return null;
    return source.split("\n").map((line) => {
      if (line.startsWith("+") && !line.startsWith("+++")) {
        return { kind: "add" as const, text: line };
      }
      if (line.startsWith("-") && !line.startsWith("---")) {
        return { kind: "del" as const, text: line };
      }
      return { kind: "ctx" as const, text: line };
    });
  });
</script>

{#if source}
  <div class="liquid-code markdown-code-block" class:liquid-code-diff={isDiff} class:markdown-code-collapsed={collapsible && !expanded} class:markdown-code-wrapped={wrapped} data-copy-hydrated="1">
    <header class="liquid-code-header markdown-code-header">
      <div class="liquid-code-meta">
        <span class="markdown-code-lang">{codeLanguageLabel(lang)}</span>
        {#if title}
          <span class="liquid-code-title">{title}</span>
        {/if}
      </div>
      <div class="markdown-code-actions">
        {#if canWrapCode(source)}
          <button type="button" class="markdown-code-wrap" aria-pressed={wrapped} title="Wrap lines" data-export-strip onclick={() => wrapped = !wrapped}>Wrap</button>
        {/if}
        {#if showCopy}
          <button type="button" class="liquid-code-copy markdown-code-copy" class:markdown-code-copy-done={copyState === "copied"} aria-label="Copy code" title={copyState === "copied" ? "Code copied" : copyState === "failed" ? "Could not copy code" : "Copy code"} onclick={copySource}>
            {@html codeCopyContent(copyState)}
          </button>
        {/if}
      </div>
    </header>
    {#if lines}
      <pre class="liquid-code-pre markdown-pre"><code class="markdown-code">{#each lines as line, index}<span class:liquid-code-add={line.kind === "add"} class:liquid-code-del={line.kind === "del"}>{line.text}{index < lines.length - 1 ? "\n" : ""}</span>{/each}</code></pre>
    {:else}
      <pre class="liquid-code-pre markdown-pre"><code bind:this={codeEl} class="markdown-code syn-code"></code></pre>
    {/if}
    {#if collapsible}
      <div class="markdown-code-footer" data-export-strip>
        <span>{lineCount} {lineCount === 1 ? "line" : "lines"}</span>
        <button type="button" class="markdown-code-expand" aria-expanded={expanded} onclick={() => expanded = !expanded}>{expanded ? "Show less" : "Show all"}</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .liquid-code-meta {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    min-width: 0;
  }

  .liquid-code-title {
    font-size: 0.72rem;
    color: rgb(var(--theme-text-tertiary));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .liquid-code-add {
    background: color-mix(in srgb, var(--color-success-500) 14%, transparent);
    color: rgb(var(--theme-success));
  }

  .liquid-code-del {
    background: color-mix(in srgb, var(--color-error-500) 14%, transparent);
    color: rgb(var(--theme-error));
  }
</style>
