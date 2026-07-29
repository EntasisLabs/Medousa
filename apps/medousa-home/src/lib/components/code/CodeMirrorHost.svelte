<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { basicSetup } from "codemirror";
  import { EditorState } from "@codemirror/state";
  import {
    EditorView,
    keymap,
  } from "@codemirror/view";
  import { indentWithTab } from "@codemirror/commands";
  import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
  import { forEachDiagnostic } from "@codemirror/lint";
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
    hasCodeEditorTabSizePreference,
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
    /** Reports the current 1-based cursor line for workspace restoration. */
    onCursorChanged?: (line: number) => void;
    /** Reports a compact selection so another workspace surface can continue here. */
    onSelectionChanged?: (selection: {
      startLine: number;
      endLine: number;
      text: string;
    }) => void;
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
    onCursorChanged,
    onSelectionChanged,
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
    const leading = value.split("\n").slice(0, 400).map((line) => line.match(/^[\t ]+/)?.[0] ?? "").filter(Boolean);
    const usesTabs = leading.some((indentation) => indentation.startsWith("\t"));
    const observedSpaces = leading
      .filter((indentation) => !indentation.includes("\t"))
      .map((indentation) => indentation.length)
      .filter((size) => size > 0);
    const languageDefault = ["javascript", "typescript", "json", "yaml", "markdown", "grapheme"].includes(resolvedLanguage) ? 2 : 4;
    const inferredSize = observedSpaces.length === 0
      ? languageDefault
      : observedSpaces.some((size) => size % 2 !== 0)
        ? 4
        : observedSpaces.some((size) => size % 4 !== 0) ? 2 : 4;
    const tabSize = hasCodeEditorTabSizePreference() ? readCodeEditorTabSize() : inferredSize;
    const indent = usesTabs ? "\t" : " ".repeat(tabSize);
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
          const selection = update.state.selection.main;
          onCursorChanged?.(
            update.state.doc.lineAt(selection.head).number,
          );
          onSelectionChanged?.({
            startLine: update.state.doc.lineAt(selection.from).number,
            endLine: update.state.doc.lineAt(selection.to).number,
            text: update.state.sliceDoc(selection.from, selection.to).slice(0, 4_000),
          });
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
      extensions.push(EditorView.theme({ ".cm-lineNumbers": { display: "none" } }));
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

  export function revealLine(lineNumber: number) {
    if (!view) return;
    const line = view.state.doc.line(
      Math.max(1, Math.min(Math.floor(lineNumber), view.state.doc.lines)),
    );
    view.dispatch({
      selection: { anchor: line.from },
      effects: EditorView.scrollIntoView(line.from, { y: "center", yMargin: 48 }),
    });
    view.focus();
  }

  export function getProblems(): Array<{
    from: number;
    to: number;
    line: number;
    severity: string;
    message: string;
  }> {
    if (!view) return [];
    const problems: Array<{
      from: number;
      to: number;
      line: number;
      severity: string;
      message: string;
    }> = [];
    forEachDiagnostic(view.state, (diagnostic, from, to) => {
      problems.push({
        from,
        to,
        line: view!.state.doc.lineAt(from).number,
        severity: diagnostic.severity,
        message: diagnostic.message,
      });
    });
    return problems;
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
