<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { basicSetup } from "codemirror";
  import { EditorState } from "@codemirror/state";
  import {
    EditorView,
    keymap,
    lineNumbers,
    highlightActiveLineGutter,
  } from "@codemirror/view";
  import { indentWithTab } from "@codemirror/commands";
  import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
  import { indentUnit } from "@codemirror/language";
  import type { LSPClient } from "@codemirror/lsp-client";
  import {
    buildCodeEditorLanguageExtensions,
    languageSupportsLsp,
    resolveCodeEditorLanguage,
    type CodeEditorLanguageId,
  } from "$lib/code/codeEditorLanguageRegistry";
  import { observeGraphemeHovers } from "$lib/grapheme/graphemeHoverEnhance";
  import {
    readCodeEditorLineNumbers,
    readCodeEditorTabSize,
    readCodeEditorWordWrap,
  } from "$lib/config/codeEditorPreferences";
  import { codeEditorFind } from "$lib/stores/codeEditorFind.svelte";

  interface Props {
    value: string;
    languageId?: CodeEditorLanguageId | string | null;
    documentUri?: string | null;
    /** LSP language id sent to the server (defaults to resolved languageId). */
    lspLanguageId?: string | null;
    client?: LSPClient | null;
    readOnly?: boolean;
    /** Bumped by parent when body is replaced externally (templates / library). */
    contentSyncKey?: string | number;
    onchange?: (value: string) => void;
    /** Fired when CM diagnostics / LSP state may have changed. */
    onProblemsChanged?: () => void;
  }

  let {
    value,
    languageId = "grapheme",
    documentUri = null,
    lspLanguageId = null,
    client = null,
    readOnly = false,
    contentSyncKey = 0,
    onchange,
    onProblemsChanged,
  }: Props = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined = $state();
  let stopHoverObserve: (() => void) | undefined;
  let applyingExternal = false;
  let syncedKey: string | number = 0;
  let onchangeRef: ((value: string) => void) | undefined;

  const resolvedLanguage = $derived(resolveCodeEditorLanguage(languageId));
  const lspLang = $derived(
    (lspLanguageId?.trim() || resolvedLanguage).toLowerCase(),
  );
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

  function buildExtensions() {
    const wrap = readCodeEditorWordWrap();
    const showLineNumbers = readCodeEditorLineNumbers();
    const tabSize = readCodeEditorTabSize();
    const indent = " ".repeat(tabSize);
    const extensions = [
      basicSetup,
      ...buildCodeEditorLanguageExtensions(resolvedLanguage),
      keymap.of([...searchKeymap, indentWithTab]),
      highlightSelectionMatches(),
      indentUnit.of(indent),
      EditorState.tabSize.of(tabSize),
      EditorState.readOnly.of(readOnly),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          emitChange(update.state.doc.toString());
        }
        if (update.docChanged || update.selectionSet) {
          codeEditorFind.syncFromView(update.view);
        }
        if (update.transactions.some((tr) => tr.effects.length > 0)) {
          onProblemsChanged?.();
        }
      }),
    ];
    if (wrap) {
      extensions.push(EditorView.lineWrapping);
    }
    if (!showLineNumbers) {
      // basicSetup includes line numbers; override by not re-adding — CM doesn't
      // easily remove them from basicSetup, so we leave them on when preferred off
      // only via a future custom setup. Prefer documenting default-on.
      void lineNumbers;
      void highlightActiveLineGutter;
    }
    if (lspEnabled && client && documentUri) {
      extensions.push(client.plugin(documentUri, lspLang));
    }
    return extensions;
  }

  onMount(() => {
    if (!host) return;
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions: buildExtensions(),
      }),
    });
    syncedKey = contentSyncKey;
    if (resolvedLanguage === "grapheme") {
      stopHoverObserve = observeGraphemeHovers(host);
    }
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

  export function getView(): EditorView | undefined {
    return view;
  }

  export function openFind() {
    codeEditorFind.show(view);
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

<div
  bind:this={host}
  class="grapheme-codemirror-host code-codemirror-host min-h-0 min-w-0 flex-1 overflow-hidden"
  role="textbox"
  tabindex="0"
  aria-label="Code editor"
  onkeydown={(e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
      e.preventDefault();
      openFind();
    }
  }}
></div>
