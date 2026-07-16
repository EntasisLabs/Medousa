<script lang="ts">
  /**
   * `code` molecule — enhanced snippet: lang badge, copy, optional diff tint.
   * Paste-first from ```code markdown (not a mistaken prose ```code fence).
   */
  import type { ArchetypeProps } from "$lib/liquid/render/types";

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

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copySource() {
    if (!source || typeof navigator === "undefined" || !navigator.clipboard?.writeText) return;
    try {
      await navigator.clipboard.writeText(source);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied = false;
        copyTimer = null;
      }, 1600);
    } catch {
      // clipboard may be denied — stay quiet
    }
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
  <div class="liquid-code" class:liquid-code-diff={isDiff}>
    <header class="liquid-code-header">
      <div class="liquid-code-meta">
        {#if lang}
          <span class="liquid-code-lang">{lang}</span>
        {/if}
        {#if title}
          <span class="liquid-code-title">{title}</span>
        {/if}
      </div>
      {#if showCopy}
        <button type="button" class="liquid-code-copy" onclick={copySource}>
          {copied ? "Copied" : "Copy"}
        </button>
      {/if}
    </header>
    {#if lines}
      <pre class="liquid-code-pre"><code
          >{#each lines as line}{#if line.kind === "add"}<span class="liquid-code-add">{line.text}
{"\n"}</span
            >{:else if line.kind === "del"}<span class="liquid-code-del">{line.text}
{"\n"}</span
            >{:else}<span class="liquid-code-ctx">{line.text}
{"\n"}</span
            >{/if}{/each}</code
        ></pre>
    {:else}
      <pre class="liquid-code-pre"><code>{source}</code></pre>
    {/if}
  </div>
{/if}

<style>
  .liquid-code {
    margin: 0;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 30%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 72%, transparent);
    overflow: hidden;
    min-width: 0;
  }

  .liquid-code-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.4rem 0.55rem 0.4rem 0.7rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 22%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 55%, transparent);
  }

  .liquid-code-meta {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    min-width: 0;
  }

  .liquid-code-lang {
    flex: 0 0 auto;
    font-size: 0.62rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-200));
    padding: 0.12rem 0.4rem;
    border-radius: 0.3rem;
    background: color-mix(in srgb, var(--color-surface-600) 55%, transparent);
  }

  .liquid-code-title {
    font-size: 0.72rem;
    color: rgb(var(--color-surface-400));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .liquid-code-copy {
    flex: 0 0 auto;
    margin: 0;
    padding: 0.2rem 0.5rem;
    border: 0;
    border-radius: 0.35rem;
    font-size: 0.68rem;
    font-weight: 600;
    color: rgb(var(--color-surface-200));
    background: color-mix(in srgb, var(--color-surface-700) 60%, transparent);
    cursor: pointer;
  }

  .liquid-code-copy:hover {
    background: color-mix(in srgb, var(--color-surface-600) 70%, transparent);
  }

  .liquid-code-pre {
    margin: 0;
    padding: 0.7rem 0.85rem;
    overflow-x: auto;
    font-size: 0.78rem;
    line-height: 1.5;
    color: rgb(var(--color-surface-100));
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }

  .liquid-code-pre code {
    font-family: inherit;
    white-space: pre;
  }

  .liquid-code-add {
    display: block;
    background: color-mix(in srgb, var(--color-success-500) 14%, transparent);
    color: rgb(var(--color-success-200));
  }

  .liquid-code-del {
    display: block;
    background: color-mix(in srgb, var(--color-error-500) 14%, transparent);
    color: rgb(var(--color-error-300));
  }

  .liquid-code-ctx {
    display: block;
  }
</style>
