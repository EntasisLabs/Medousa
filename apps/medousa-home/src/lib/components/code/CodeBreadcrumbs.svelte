<script lang="ts">
  import { ChevronRight } from "@lucide/svelte";
  import {
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

  const segments = $derived(pathBreadcrumbSegments(path));
</script>

<nav class="code-breadcrumbs flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto" aria-label="File location">
  {#each segments as segment, index (segment.path)}
    {#if index > 0}
      <ChevronRight size={10} class="shrink-0 text-content-faint" aria-hidden="true" />
    {/if}
    <button
      type="button"
      class="code-breadcrumb-seg max-w-36 shrink-0 truncate rounded px-1 py-0.5 text-left font-mono text-[11px] {segment.isFile
        ? 'text-surface-100 hover:bg-surface-800'
        : 'text-content-tertiary hover:bg-surface-800 hover:text-surface-200'}"
      title={segment.path}
      onclick={() => onPathSegment?.(segment.path, segment.isFile)}
    >{segment.label}</button>
  {/each}
  {#each symbols as symbol, index (`${symbol.name}:${symbol.line}:${index}`)}
    <ChevronRight size={10} class="shrink-0 text-content-faint" aria-hidden="true" />
    <button
      type="button"
      class="code-breadcrumb-seg max-w-40 shrink-0 truncate rounded px-1 py-0.5 text-left text-[11px] text-content-link/80 hover:bg-surface-800 hover:text-primary-200"
      title={`Go to ${symbol.name}`}
      onclick={() => onSymbol?.(symbol.line)}
    >{symbol.name}</button>
  {/each}
</nav>
