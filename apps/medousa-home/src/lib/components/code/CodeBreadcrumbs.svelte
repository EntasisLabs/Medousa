<script lang="ts">
  import { ChevronRight } from "@lucide/svelte";
  import {
    collapsePathBreadcrumbs,
    collapseSymbolTrail,
    pathBreadcrumbSegments,
    type CodeBreadcrumbSymbol,
  } from "$lib/code/codeDocumentSymbols";

  interface Props {
    path: string;
    symbols?: CodeBreadcrumbSymbol[];
    onPathSegment?: (path: string, isFile: boolean) => void;
    onSymbol?: (line: number) => void;
  }

  let { path, symbols = [], onPathSegment, onSymbol }: Props = $props();

  const segments = $derived(collapsePathBreadcrumbs(pathBreadcrumbSegments(path)));
  const visibleSymbols = $derived(collapseSymbolTrail(symbols, 1));
  const fullPathTitle = $derived(path);
</script>

<nav
  class="code-breadcrumbs flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto"
  aria-label="File location"
  title={fullPathTitle}
>
  {#each segments as segment, index (segment.ellipsis ? `…:${index}` : segment.path)}
    {#if index > 0}
      <ChevronRight size={11} class="code-breadcrumb-chevron" aria-hidden="true" />
    {/if}
    {#if segment.ellipsis}
      <span class="code-breadcrumb-ellipsis" aria-hidden="true">…</span>
    {:else}
      <button
        type="button"
        class="code-breadcrumb-seg"
        class:code-breadcrumb-seg--file={segment.isFile}
        class:code-breadcrumb-seg--dir={!segment.isFile}
        title={segment.path}
        onclick={() => onPathSegment?.(segment.path, segment.isFile)}
      >{segment.label}</button>
    {/if}
  {/each}
  {#each visibleSymbols as symbol, index (`${symbol.name}:${symbol.line}:${index}`)}
    <ChevronRight size={11} class="code-breadcrumb-chevron" aria-hidden="true" />
    <button
      type="button"
      class="code-breadcrumb-seg code-breadcrumb-seg--symbol"
      title={`Go to ${symbol.name}`}
      onclick={() => onSymbol?.(symbol.line)}
    >{symbol.name}</button>
  {/each}
</nav>

<style>
  .code-breadcrumbs {
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(.code-breadcrumb-chevron) {
    flex-shrink: 0;
    color: color-mix(in srgb, rgb(var(--theme-text)) 35%, transparent);
  }

  .code-breadcrumb-ellipsis {
    flex-shrink: 0;
    padding: 0 0.15rem;
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 55%,
      rgb(var(--theme-text-secondary))
    );
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 400;
    line-height: 1.2;
  }

  .code-breadcrumb-seg {
    max-width: 9rem;
    flex-shrink: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    padding: 0.1rem 0.25rem;
    text-align: left;
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 400;
    letter-spacing: 0;
    line-height: 1.2;
    cursor: pointer;
  }

  .code-breadcrumb-seg--dir {
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 62%,
      rgb(var(--theme-text-secondary))
    );
  }

  .code-breadcrumb-seg--file {
    flex-shrink: 0;
    max-width: 12rem;
    color: rgb(var(--theme-text));
    font-weight: 500;
  }

  .code-breadcrumb-seg--symbol {
    flex-shrink: 0;
    max-width: 10rem;
    color: color-mix(
      in srgb,
      rgb(var(--theme-link)) 75%,
      rgb(var(--theme-text))
    );
  }

  .code-breadcrumb-seg:hover {
    background: rgb(var(--color-surface-800) / 0.55);
    color: rgb(var(--theme-text));
  }
</style>
