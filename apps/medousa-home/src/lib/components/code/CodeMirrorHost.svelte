<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { basicSetup } from "codemirror";
  import { EditorState } from "@codemirror/state";
  import { EditorView, keymap } from "@codemirror/view";
  import { indentWithTab } from "@codemirror/commands";
  import type { LSPClient } from "@codemirror/lsp-client";
  import {
    buildCodeEditorLanguageExtensions,
    languageSupportsLsp,
    resolveCodeEditorLanguage,
    type CodeEditorLanguageId,
  } from "$lib/code/codeEditorLanguageRegistry";
  import { observeGraphemeHovers } from "$lib/grapheme/graphemeHoverEnhance";

  interface Props {
    value: string;
    languageId?: CodeEditorLanguageId | string | null;
    documentUri?: string | null;
    client?: LSPClient | null;
    readOnly?: boolean;
    onchange?: (value: string) => void;
  }

  let {
    value,
    languageId = "grapheme",
    documentUri = null,
    client = null,
    readOnly = false,
    onchange,
  }: Props = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  let stopHoverObserve: (() => void) | undefined;

  const resolvedLanguage = $derived(resolveCodeEditorLanguage(languageId));
  const lspEnabled = $derived(
    languageSupportsLsp(resolvedLanguage) && Boolean(client && documentUri),
  );

  onMount(() => {
    if (!host) return;
    const extensions = [
      basicSetup,
      ...buildCodeEditorLanguageExtensions(resolvedLanguage),
      keymap.of([indentWithTab]),
      EditorView.lineWrapping,
      EditorState.readOnly.of(readOnly),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onchange?.(update.state.doc.toString());
        }
      }),
    ];
    if (lspEnabled && client && documentUri) {
      extensions.push(client.plugin(documentUri, "grapheme"));
    }
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions,
      }),
    });
    if (resolvedLanguage === "grapheme") {
      stopHoverObserve = observeGraphemeHovers(host);
    }
  });

  onDestroy(() => {
    stopHoverObserve?.();
    stopHoverObserve = undefined;
    view?.destroy();
    view = undefined;
  });

  export function insertText(text: string) {
    if (!view || !text) return;
    const { from, to } = view.state.selection.main;
    view.dispatch({
      changes: { from, to, insert: text },
      selection: { anchor: from + text.length },
    });
    onchange?.(view.state.doc.toString());
  }

  export function focusEditor() {
    view?.focus();
  }

  $effect(() => {
    if (!view || view.state.doc.toString() === value) return;
    view.dispatch({
      changes: {
        from: 0,
        to: view.state.doc.length,
        insert: value,
      },
    });
  });
</script>

<div bind:this={host} class="code-codemirror-host min-h-0 flex-1"></div>
