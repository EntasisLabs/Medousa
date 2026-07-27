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
    /** Bumped by parent when body is replaced externally (templates / library). */
    contentSyncKey?: string | number;
    onchange?: (value: string) => void;
  }

  let {
    value,
    languageId = "grapheme",
    documentUri = null,
    client = null,
    readOnly = false,
    contentSyncKey = 0,
    onchange,
  }: Props = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined = $state();
  let stopHoverObserve: (() => void) | undefined;
  let applyingExternal = false;
  let syncedKey: string | number = contentSyncKey;
  let onchangeRef: ((value: string) => void) | undefined = onchange;

  const resolvedLanguage = $derived(resolveCodeEditorLanguage(languageId));
  const lspEnabled = $derived(
    languageSupportsLsp(resolvedLanguage) && Boolean(client && documentUri),
  );

  $effect(() => {
    onchangeRef = onchange;
  });

  function emitChange(next: string) {
    if (applyingExternal) return;
    onchangeRef?.(next);
  }

  function applyExternalValue(next: string) {
    if (!view || view.state.doc.toString() === next) return;
    applyingExternal = true;
    try {
      view.dispatch({
        changes: {
          from: 0,
          to: view.state.doc.length,
          insert: next,
        },
      });
    } finally {
      applyingExternal = false;
    }
  }

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
          emitChange(update.state.doc.toString());
        }
      }),
    ];
    if (lspEnabled && client && documentUri) {
      extensions.push(client.plugin(documentUri, "grapheme"));
    }
    // Read value at mount time (may still be "" if parent hydrates next tick).
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions,
      }),
    });
    syncedKey = contentSyncKey;
    if (resolvedLanguage === "grapheme") {
      stopHoverObserve = observeGraphemeHovers(host);
    }
    // Same race Notes hits: mount with "" then parent fills body.
    if (value && view.state.doc.toString() !== value) {
      applyExternalValue(value);
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
    emitChange(view.state.doc.toString());
  }

  export function focusEditor() {
    view?.focus();
  }

  /**
   * Keep CM doc aligned with the parent `value`.
   * Handles external template/library loads and mount-before-hydrate.
   */
  $effect(() => {
    if (!view) return;
    const next = value;
    const key = contentSyncKey;
    const current = view.state.doc.toString();
    if (current === next) {
      syncedKey = key;
      return;
    }
    if (key !== syncedKey || (current.length === 0 && next.length > 0)) {
      syncedKey = key;
      applyExternalValue(next);
    }
  });
</script>

<div bind:this={host} class="grapheme-codemirror-host code-codemirror-host min-h-0 min-w-0 flex-1 overflow-hidden"></div>
