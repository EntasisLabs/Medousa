<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { basicSetup } from "codemirror";
  import { EditorState } from "@codemirror/state";
  import { EditorView, keymap } from "@codemirror/view";
  import { indentWithTab } from "@codemirror/commands";
  import type { LSPClient } from "@codemirror/lsp-client";
  import {
    graphemeEditorTheme,
    graphemeLanguageSupport,
  } from "$lib/grapheme/graphemeEditorTheme";
  import { graphemeHostCompletions } from "$lib/grapheme/graphemeHostCompletions";
  import { observeGraphemeHovers } from "$lib/grapheme/graphemeHoverEnhance";

  interface Props {
    value: string;
    documentUri: string;
    client: LSPClient | null;
    readOnly?: boolean;
    onchange?: (value: string) => void;
  }

  let {
    value,
    documentUri,
    client,
    readOnly = false,
    onchange,
  }: Props = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  let stopHoverObserve: (() => void) | undefined;

  onMount(() => {
    if (!host) return;
    const extensions = [
      basicSetup,
      graphemeEditorTheme,
      graphemeLanguageSupport,
      graphemeHostCompletions(),
      keymap.of([indentWithTab]),
      EditorView.lineWrapping,
      EditorState.readOnly.of(readOnly),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onchange?.(update.state.doc.toString());
        }
      }),
    ];
    if (client) {
      extensions.push(client.plugin(documentUri, "grapheme"));
    }
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions,
      }),
    });
    stopHoverObserve = observeGraphemeHovers(host);
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

<div bind:this={host} class="grapheme-codemirror-host min-h-0 flex-1"></div>
