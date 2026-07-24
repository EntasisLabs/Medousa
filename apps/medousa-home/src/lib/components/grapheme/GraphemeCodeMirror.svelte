<script lang="ts">
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import type { LSPClient } from "@codemirror/lsp-client";

  interface Props {
    value: string;
    documentUri: string;
    client: LSPClient | null;
    readOnly?: boolean;
    onchange?: (value: string) => void;
  }

  let { value, documentUri, client, readOnly = false, onchange }: Props = $props();

  let host = $state<CodeMirrorHost | undefined>();

  export function insertText(text: string) {
    host?.insertText(text);
  }

  export function focusEditor() {
    host?.focusEditor();
  }
</script>

<CodeMirrorHost
  bind:this={host}
  {value}
  languageId="grapheme"
  {documentUri}
  {client}
  {readOnly}
  {onchange}
/>
