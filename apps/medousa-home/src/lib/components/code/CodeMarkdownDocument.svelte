<script lang="ts">
  import { onDestroy, tick, untrack, type Snippet } from "svelte";
  import { Columns2, Code2, Eye, FilePenLine } from "@lucide/svelte";
  import type CodeMirrorHost from "./CodeMirrorHost.svelte";
  import CodeMarkdownPreview from "./CodeMarkdownPreview.svelte";
  import CodeMarkdownWriter from "./CodeMarkdownWriter.svelte";
  import { codeWorkspace, type CodeDocumentTab } from "$lib/stores/codeWorkspace.svelte";
  import { ProjectMarkdownImages, type CodeMarkdownMode } from "$lib/code/codeMarkdownDocument";
  import type { CodeFindState } from "$lib/code/codeFindController.svelte";

  let { tab, editor, readOnly, findState, source, onchange, onOpenLocation }: {
    tab: CodeDocumentTab; editor: CodeMirrorHost | undefined; readOnly: boolean; findState?: CodeFindState;
    source: Snippet; onchange: (value: string) => void; onOpenLocation: (path: string, line: number) => void;
  } = $props();
  const isMarkdown = $derived(tab.language === "markdown" && !tab.preview);
  const mode = $derived(isMarkdown ? tab.markdownMode ?? "source" : "source");
  let writer = $state<CodeMarkdownWriter>();
  let writerOpened = $state(false);
  let previewOpened = $state(false);
  const images = untrack(() => new ProjectMarkdownImages(tab.work_id, tab.path));
  onDestroy(() => images.dispose());

  async function choose(next: CodeMarkdownMode) {
    editor?.flushChanges();
    if (findState && next !== "source" && next !== "split") findState.open = false;
    if (next === "markdown") writerOpened = true;
    if (next === "preview" || next === "split") previewOpened = true;
    codeWorkspace.patch(tab.tabId, { markdownMode: next });
    await tick();
    if (next === "markdown") writer?.focusEditor();
    else if (next !== "preview") { editor?.getView()?.requestMeasure(); editor?.focusEditor(); }
  }

  $effect(() => {
    if (mode === "markdown") writerOpened = true;
    if (mode === "preview" || mode === "split") previewOpened = true;
    if (findState?.open && mode !== "source" && mode !== "split") codeWorkspace.patch(tab.tabId, { markdownMode: "source" });
  });

  function keydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && (event.key.toLowerCase() === "f" || event.key.toLowerCase() === "h") && mode !== "source" && mode !== "split") {
      event.preventDefault();
      const replace = event.key.toLowerCase() === "h" || event.altKey;
      void choose("source").then(() => { if (replace) editor?.openReplace(); else editor?.openFind(); });
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="code-document flex h-full min-h-0 flex-col" onkeydown={keydown}>
  {#if isMarkdown}
    <div class="flex shrink-0 items-center justify-end gap-0.5 border-b border-surface-600/30 px-2 py-1" role="toolbar" aria-label="Markdown views">
      {#each [{ value: 'source', label: 'Source', title: 'Edit Markdown source', Icon: Code2 }, { value: 'markdown', label: 'Markdown', title: 'Open in Markdown editor', Icon: FilePenLine }, { value: 'preview', label: 'Preview', title: 'Preview the current draft', Icon: Eye }, { value: 'split', label: 'Split', title: 'Source and live Markdown preview', Icon: Columns2 }] as item (item.value)}
        <button type="button" class="markdown-view inline-flex items-center gap-1.5 rounded px-2 py-1 text-chrome-sm text-content-secondary hover:bg-surface-700/40" class:active={mode === item.value} aria-label={item.title} title={item.title} aria-pressed={mode === item.value} onclick={() => void choose(item.value as CodeMarkdownMode)}>
          <item.Icon size={13} /><span>{item.label}</span>
        </button>
      {/each}
    </div>
  {/if}
  <div class="document-panes flex min-h-0 flex-1 overflow-hidden" class:split={mode === 'split'}>
    <div class="source-pane min-h-0 min-w-0 flex-1" hidden={mode === 'markdown' || mode === 'preview'}>
      {@render source()}
    </div>
    {#if isMarkdown && writerOpened}
      <div class="writer-pane min-h-0 min-w-0 flex-1" hidden={mode !== 'markdown'}>
        <CodeMarkdownWriter bind:this={writer} value={tab.draft} path={tab.path} workId={tab.work_id} disabled={readOnly} {images} {onOpenLocation} onchange={(value) => { if (readOnly) return; editor?.replaceValue(value); onchange(value); }} />
      </div>
    {/if}
    {#if isMarkdown && previewOpened}
      <div class="preview-pane min-h-0 min-w-0 flex-1" hidden={mode !== 'preview' && mode !== 'split'} class:border-l={mode === 'split'}>
        <CodeMarkdownPreview content={tab.draft} path={tab.path} workId={tab.work_id} {images} {onOpenLocation} />
      </div>
    {/if}
  </div>
</div>

<style>
  .source-pane[hidden], .writer-pane[hidden], .preview-pane[hidden] { display: none; }
  .markdown-view.active { background: rgb(var(--color-primary-500) / .15); color: rgb(var(--color-primary-200)); }
  .preview-pane { border-color: rgb(var(--color-surface-600) / .3); }
  .split > .source-pane, .split > .preview-pane { flex-basis: 50%; }
  @media (max-width: 700px) { .document-panes.split { flex-direction: column; } .split > .preview-pane { border-left: 0; border-top: 1px solid rgb(var(--color-surface-600) / .3); } }
</style>
