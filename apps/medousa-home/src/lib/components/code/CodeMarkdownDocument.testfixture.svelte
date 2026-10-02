<script lang="ts">
  import CodeMarkdownDocument from "./CodeMarkdownDocument.svelte";
  import CodeMirrorHost from "./CodeMirrorHost.svelte";
  import { codeWorkspace, type CodeDocumentTab } from "$lib/stores/codeWorkspace.svelte";
  import { CodeFindState } from "$lib/code/codeFindController.svelte";
  let { readOnly = false, syntaxTheme = "dark-plus", onDraft = (_value: string) => {} }: { readOnly?: boolean; syntaxTheme?: string; onDraft?: (value: string) => void } = $props();
  let editor = $state<CodeMirrorHost>();
  const findState = new CodeFindState();
  const tab = $derived(codeWorkspace.tabs[0] as CodeDocumentTab);
  function changed(value: string) { codeWorkspace.patch(tab.tabId, { draft: value }); onDraft(value); }
  export function getValue() { return editor?.getValue(); }
</script>

<div style="height: 560px; width: 100%;">
  {#if tab}
    <CodeMarkdownDocument {tab} {editor} {readOnly} {findState} onchange={changed} onOpenLocation={(path) => onDraft(`open:${path}`)}>
      {#snippet source()}
        <CodeMirrorHost bind:this={editor} value={tab.draft} languageId={tab.language} contentSyncKey={tab.syncKey} {readOnly} {findState} {syntaxTheme} onchange={changed} />
      {/snippet}
    </CodeMarkdownDocument>
  {/if}
</div>
<output data-testid="document-draft" style="white-space:pre-wrap">{tab?.draft}</output>
