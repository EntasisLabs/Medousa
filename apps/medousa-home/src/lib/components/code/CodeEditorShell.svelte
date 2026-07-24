<script lang="ts">
  import type { Snippet } from "svelte";
  import ScriptEditorTabStrip from "$lib/components/automations/ScriptEditorTabStrip.svelte";

  interface Props {
    workbenchMode?: boolean;
    pieceLanded?: boolean;
    showHeader?: boolean;
    showFooter?: boolean;
    showSidePane?: boolean;
    showTabs?: boolean;
    useDefaultTabStrip?: boolean;
    class?: string;
    header?: Snippet;
    toolbar?: Snippet;
    tabs?: Snippet;
    beforeEditor?: Snippet;
    editor: Snippet;
    sidePane?: Snippet;
    statusBar?: Snippet;
  }

  let {
    workbenchMode = false,
    pieceLanded = false,
    showHeader = true,
    showFooter = true,
    showSidePane = true,
    showTabs = true,
    useDefaultTabStrip = true,
    class: className = "",
    header,
    toolbar,
    tabs,
    beforeEditor,
    editor,
    sidePane,
    statusBar,
  }: Props = $props();
</script>

<div
  class="code-editor-shell flex min-h-0 flex-1 flex-col overflow-hidden {pieceLanded
    ? 'scripts-piece-landed'
    : ''} {className}"
>
  {#if !workbenchMode && showHeader}
    <header class="code-editor-shell-header shrink-0 border-b border-surface-500/40 px-4 py-3">
      {#if header}
        {@render header()}
      {/if}
      {#if toolbar}
        <div>{@render toolbar()}</div>
      {/if}
      {#if showTabs}
        <div class="mt-3 border-b border-surface-600/50 pb-px">
          {#if tabs}
            {@render tabs()}
          {:else if useDefaultTabStrip}
            <ScriptEditorTabStrip />
          {/if}
        </div>
      {/if}
    </header>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      {#if beforeEditor}
        {@render beforeEditor()}
      {/if}
      {@render editor()}
    </div>

    {#if !workbenchMode && showSidePane && sidePane}
      <aside
        class="code-editor-shell-side w-[min(360px,34%)] shrink-0 overflow-y-auto border-l border-surface-500/40 px-4 py-4"
      >
        {@render sidePane()}
      </aside>
    {/if}
  </div>

  {#if !workbenchMode && showFooter && statusBar}
    <footer class="code-editor-shell-footer workshop-status shrink-0 border-t border-surface-500/40 px-4 py-2">
      {@render statusBar()}
    </footer>
  {/if}
</div>
