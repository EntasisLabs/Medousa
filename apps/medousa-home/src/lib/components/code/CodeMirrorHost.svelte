<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { basicSetup } from "codemirror";
  import { Compartment, EditorState, type Extension } from "@codemirror/state";
  import {
    EditorView,
    GutterMarker,
    gutter,
    keymap,
  } from "@codemirror/view";
  import { indentWithTab } from "@codemirror/commands";
  import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
  import { forEachDiagnostic } from "@codemirror/lint";
  import { foldGutter, indentUnit } from "@codemirror/language";
  import { jumpToDefinition, type LSPClient } from "@codemirror/lsp-client";
  import {
    buildCodeEditorLanguageExtensions,
    languageSupportsLsp,
    resolveCodeEditorLanguage,
    type CodeEditorLanguageId,
  } from "$lib/code/codeEditorLanguageRegistry";
  import { observeGraphemeHovers } from "$lib/grapheme/graphemeHoverEnhance";
  import {
    readCodeEditorFontSize,
    readCodeEditorIndentGuides,
    readCodeEditorLineNumbers,
    readCodeEditorTabSize,
    readCodeEditorWordWrap,
    hasCodeEditorTabSizePreference,
  } from "$lib/config/codeEditorPreferences";
  import { codeEditorFind } from "$lib/stores/codeEditorFind.svelte";

  /** Medousa chrome for the coding host — Notes care, gutters kept for code. */
  const codeEditorChromeTheme = EditorView.theme(
    {
      "&": {
        height: "100%",
        width: "100%",
        maxWidth: "100%",
        fontSize: "inherit",
        color: "rgb(var(--color-surface-100))",
        backgroundColor: "transparent",
      },
      ".cm-scroller": {
        overflow: "auto",
        maxWidth: "100%",
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
        fontSize: "var(--code-editor-font-size, 13px)",
        lineHeight: "1.55",
      },
      ".cm-content": {
        padding: "0.35rem 0",
        caretColor: "rgb(var(--color-primary-200))",
        color: "rgb(var(--color-surface-100))",
      },
      ".cm-cursor, .cm-dropCursor": {
        borderLeftColor: "rgb(var(--color-primary-200))",
        borderLeftWidth: "2px",
      },
      "&.cm-focused .cm-cursor": {
        borderLeftColor: "rgb(var(--color-primary-100))",
      },
      ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
        backgroundColor: "rgb(var(--color-primary-500) / 0.28) !important",
      },
      ".cm-gutters": {
        backgroundColor: "transparent",
        border: "none",
        color: "rgb(var(--color-surface-600))",
        minWidth: "2.25rem",
      },
      ".cm-lineNumbers .cm-gutterElement": {
        padding: "0 0.5rem 0 0.2rem",
        minWidth: "1.85rem",
      },
      ".cm-activeLine": {
        backgroundColor: "rgb(var(--color-surface-900) / 0.45)",
      },
      ".cm-activeLineGutter": {
        backgroundColor: "rgb(var(--color-surface-900) / 0.35)",
        color: "rgb(var(--color-surface-400))",
      },
      ".cm-selectionMatch": {
        backgroundColor: "rgb(var(--color-primary-500) / 0.18)",
      },
      ".cm-searchMatch": {
        backgroundColor: "rgb(var(--color-warning-400) / 0.35)",
      },
      ".cm-searchMatch.cm-searchMatch-selected": {
        backgroundColor: "rgb(var(--color-warning-300) / 0.55)",
      },
      "&.cm-focused": {
        outline: "none",
      },
    },
    { dark: true },
  );

  class ReviewMarker extends GutterMarker {
    kind: string;

    constructor(kind: string) {
      super();
      this.kind = kind;
    }

    toDOM() {
      const marker = document.createElement("span");
      marker.className = `code-review-gutter-marker code-review-gutter-${this.kind}`;
      marker.title = this.kind === "deleted" ? "Lines removed near here" : "Line changed in this project";
      return marker;
    }
  }

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
    /** Reports 1-based cursor line/column for status bar and workspace restoration. */
    onCursorChanged?: (cursor: {
      line: number;
      totalLines: number;
      column: number;
    }) => void;
    /** Reports a compact selection so another workspace surface can continue here. */
    onSelectionChanged?: (selection: {
      startLine: number;
      endLine: number;
      text: string;
    }) => void;
    /** Fired when CM diagnostics / LSP state may have changed. */
    onProblemsChanged?: () => void;
    /** Editor right-click — parent shows Medousa context menu. */
    onContextMenu?: (event: MouseEvent) => void;
    /** Baseline-to-reviewed lines shown as quiet source-control markers. */
    changedLines?: Array<{ line: number; kind: string }>;
    conventionIndentStyle?: "space" | "tab" | null;
    conventionTabSize?: number | null;
    wordWrap?: boolean;
    showLineNumbers?: boolean;
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
    onContextMenu,
    changedLines = [],
    conventionIndentStyle = null,
    conventionTabSize = null,
    wordWrap = readCodeEditorWordWrap(),
    showLineNumbers = readCodeEditorLineNumbers(),
  }: Props = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined = $state();
  let stopHoverObserve: (() => void) | undefined;
  let applyingExternal = false;
  let syncedKey: string | number = 0;
  let onchangeRef: ((value: string) => void) | undefined;
  let onCursorChangedRef: Props["onCursorChanged"];
  let onSelectionChangedRef: Props["onSelectionChanged"];
  let onProblemsChangedRef: Props["onProblemsChanged"];
  let onContextMenuRef: ((event: MouseEvent) => void) | undefined;
  let changeTimer: ReturnType<typeof setTimeout> | undefined;
  let pendingChangeCallback: ((value: string) => void) | undefined;
  let telemetryFrame: number | undefined;

  const languageCompartment = new Compartment();
  const indentationCompartment = new Compartment();
  const readOnlyCompartment = new Compartment();
  const lspCompartment = new Compartment();
  const wrapCompartment = new Compartment();
  const lineNumbersCompartment = new Compartment();
  const reviewCompartment = new Compartment();

  const resolvedLanguage = $derived(resolveCodeEditorLanguage(languageId));
  const lspLang = $derived(
    (lspLanguageId?.trim() || resolvedLanguage).toLowerCase(),
  );
  const lspEnabled = $derived(
    languageSupportsLsp(resolvedLanguage) && Boolean(client && documentUri),
  );

  $effect(() => {
    onchangeRef = onchange;
    onCursorChangedRef = onCursorChanged;
    onSelectionChangedRef = onSelectionChanged;
    onProblemsChangedRef = onProblemsChanged;
  });

  $effect(() => {
    onContextMenuRef = onContextMenu;
  });

  function scheduleChange() {
    if (applyingExternal) return;
    if (changeTimer) clearTimeout(changeTimer);
    pendingChangeCallback = onchangeRef;
    changeTimer = setTimeout(() => {
      changeTimer = undefined;
      const callback = pendingChangeCallback;
      pendingChangeCallback = undefined;
      if (view) callback?.(view.state.doc.toString());
    }, 120);
  }

  function flushChange() {
    if (!changeTimer) return;
    clearTimeout(changeTimer);
    changeTimer = undefined;
    const callback = pendingChangeCallback;
    pendingChangeCallback = undefined;
    if (view) callback?.(view.state.doc.toString());
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

  function reportCursor(state: EditorState) {
    const selection = state.selection.main;
    const line = state.doc.lineAt(selection.head);
    onCursorChangedRef?.({
      line: line.number,
      totalLines: state.doc.lines,
      column: selection.head - line.from + 1,
    });
  }

  function indentationExtensions(content: string): Extension {
    const leading = content.split("\n").slice(0, 400).map((line) => line.match(/^[\t ]+/)?.[0] ?? "").filter(Boolean);
    const usesTabs = conventionIndentStyle
      ? conventionIndentStyle === "tab"
      : leading.some((indentation) => indentation.startsWith("\t"));
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
    const tabSize = hasCodeEditorTabSizePreference()
      ? readCodeEditorTabSize()
      : conventionTabSize ?? inferredSize;
    const indent = usesTabs ? "\t" : " ".repeat(tabSize);
    return [indentUnit.of(indent), EditorState.tabSize.of(tabSize)];
  }

  function lspExtensions(): Extension {
    return lspEnabled && client && documentUri
      ? client.plugin(documentUri, lspLang)
      : [];
  }

  function reviewExtensions(): Extension {
    if (changedLines.length === 0) return [];
    const markers = new Map(changedLines.map((change) => [change.line, change.kind]));
    return gutter({
      class: "code-review-gutter",
      lineMarker(editorView, line) {
        const kind = markers.get(editorView.state.doc.lineAt(line.from).number);
        return kind ? new ReviewMarker(kind) : null;
      },
    });
  }

  function buildExtensions(): Extension[] {
    const showIndentGuides = readCodeEditorIndentGuides();
    return [
      basicSetup,
      codeEditorChromeTheme,
      foldGutter(),
      EditorState.allowMultipleSelections.of(true),
      languageCompartment.of(buildCodeEditorLanguageExtensions(resolvedLanguage)),
      keymap.of([...searchKeymap, indentWithTab]),
      highlightSelectionMatches(),
      indentationCompartment.of(indentationExtensions(value)),
      readOnlyCompartment.of(EditorState.readOnly.of(readOnly)),
      lspCompartment.of(lspExtensions()),
      wrapCompartment.of(wordWrap ? EditorView.lineWrapping : []),
      lineNumbersCompartment.of(
        showLineNumbers
          ? []
          : EditorView.theme({ ".cm-lineNumbers": { display: "none" } }),
      ),
      reviewCompartment.of(reviewExtensions()),
      showIndentGuides
        ? EditorView.theme({
            ".cm-line": {
              backgroundImage:
                "repeating-linear-gradient(to right, transparent 0, transparent calc(1ch - 1px), rgb(var(--color-surface-500) / 0.12) calc(1ch - 1px), rgb(var(--color-surface-500) / 0.12) 1ch)",
              backgroundSize: "2ch 100%",
              backgroundPosition: "0 0",
            },
          })
        : [],
      EditorView.domEventHandlers({
        contextmenu(event) {
          if (!onContextMenuRef) return false;
          event.preventDefault();
          onContextMenuRef(event);
          return true;
        },
      }),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          scheduleChange();
        }
        if (update.docChanged || update.selectionSet) {
          codeEditorFind.syncFromView(update.view);
          if (telemetryFrame !== undefined) cancelAnimationFrame(telemetryFrame);
          telemetryFrame = requestAnimationFrame(() => {
            telemetryFrame = undefined;
            reportCursor(update.state);
            const selection = update.state.selection.main;
            onSelectionChangedRef?.({
              startLine: update.state.doc.lineAt(selection.from).number,
              endLine: update.state.doc.lineAt(selection.to).number,
              text: update.state.sliceDoc(
                selection.from,
                Math.min(selection.to, selection.from + 4_000),
              ),
            });
          });
        }
        if (update.transactions.some((tr) => tr.effects.length > 0)) {
          onProblemsChangedRef?.();
        }
      }),
    ];
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
    reportCursor(view.state);
    if (resolvedLanguage === "grapheme") {
      stopHoverObserve = observeGraphemeHovers(host);
    }
    if (value && view.state.doc.toString() !== value) {
      applyExternalValue(value);
    }
  });

  onDestroy(() => {
    flushChange();
    if (telemetryFrame !== undefined) cancelAnimationFrame(telemetryFrame);
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
  }

  export function getValue(): string {
    return view?.state.doc.toString() ?? value;
  }

  export function flushChanges() {
    flushChange();
  }

  export function focusEditor() {
    view?.focus();
  }

  export function getCursorPosition(): { line: number; character: number } {
    if (!view) return { line: 0, character: 0 };
    const head = view.state.selection.main.head;
    const line = view.state.doc.lineAt(head);
    return { line: line.number - 1, character: head - line.from };
  }

  export function getSelectedWord(): string {
    if (!view) return "";
    const selection = view.state.selection.main;
    if (!selection.empty) {
      return view.state.sliceDoc(selection.from, selection.to).trim();
    }
    const head = selection.head;
    const line = view.state.doc.lineAt(head);
    const text = line.text;
    const offset = head - line.from;
    const left = text.slice(0, offset).match(/[\w$]+$/)?.[0] ?? "";
    const right = text.slice(offset).match(/^[\w$]+/)?.[0] ?? "";
    return `${left}${right}`;
  }

  export function getView(): EditorView | undefined {
    return view;
  }

  export function goToDefinition(): boolean {
    if (!view) return false;
    return jumpToDefinition(view);
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

  export function applyLanguageEdits(edits: Array<{
    newText?: string;
    range?: {
      start?: { line?: number; character?: number };
      end?: { line?: number; character?: number };
    };
  }>) {
    if (!view || edits.length === 0) return;
    const offset = (position: { line?: number; character?: number } | undefined) => {
      const lineNumber = Math.max(
        1,
        Math.min((position?.line ?? 0) + 1, view!.state.doc.lines),
      );
      const line = view!.state.doc.line(lineNumber);
      return Math.min(line.to, line.from + Math.max(0, position?.character ?? 0));
    };
    view.dispatch({
      changes: edits
        .map((edit) => ({
          from: offset(edit.range?.start),
          to: offset(edit.range?.end),
          insert: edit.newText ?? "",
        }))
        .sort((a, b) => a.from - b.from),
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

  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: languageCompartment.reconfigure(
        buildCodeEditorLanguageExtensions(resolvedLanguage),
      ),
    });
    stopHoverObserve?.();
    stopHoverObserve = resolvedLanguage === "grapheme" && host
      ? observeGraphemeHovers(host)
      : undefined;
  });

  $effect(() => {
    if (!view) return;
    void conventionIndentStyle;
    void conventionTabSize;
    void resolvedLanguage;
    view.dispatch({
      effects: indentationCompartment.reconfigure(
        indentationExtensions(untrack(() => view?.state.doc.toString() ?? value)),
      ),
    });
  });

  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: readOnlyCompartment.reconfigure(EditorState.readOnly.of(readOnly)),
    });
  });

  $effect(() => {
    if (!view) return;
    view.dispatch({ effects: lspCompartment.reconfigure(lspExtensions()) });
  });

  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: wrapCompartment.reconfigure(wordWrap ? EditorView.lineWrapping : []),
    });
  });

  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: lineNumbersCompartment.reconfigure(
        showLineNumbers
          ? []
          : EditorView.theme({ ".cm-lineNumbers": { display: "none" } }),
      ),
    });
  });

  $effect(() => {
    if (!view) return;
    void changedLines;
    view.dispatch({ effects: reviewCompartment.reconfigure(reviewExtensions()) });
  });
</script>

<div
  bind:this={host}
  class="grapheme-codemirror-host code-codemirror-host h-full min-h-0 min-w-0 flex-1 overflow-hidden"
  style={`--code-editor-font-size: ${readCodeEditorFontSize()}px`}
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

<style>
  :global(.code-review-gutter) {
    width: 4px;
    background: transparent;
  }

  :global(.code-review-gutter-marker) {
    display: block;
    width: 3px;
    height: 100%;
    min-height: 1.15rem;
    border-radius: 999px;
    background: rgb(var(--color-primary-400));
  }

  :global(.code-review-gutter-deleted) {
    height: 3px;
    min-height: 3px;
    margin-top: 0.55rem;
    background: rgb(251 113 133);
  }

  /* Medousa-skinned CodeMirror find/replace panel */
  :global(.code-codemirror-host .cm-panel.cm-search) {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
    padding: 0.4rem 0.55rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-950) / 0.94);
    color: rgb(var(--color-surface-200));
    font-size: 0.7rem;
  }

  :global(.code-codemirror-host .cm-panel.cm-search input[type="text"]),
  :global(.code-codemirror-host .cm-panel.cm-search input:not([type])) {
    min-width: 8rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.45);
    border-radius: 0.3rem;
    background: rgb(var(--color-surface-900));
    padding: 0.2rem 0.45rem;
    color: rgb(var(--color-surface-100));
    outline: none;
  }

  :global(.code-codemirror-host .cm-panel.cm-search input[type="text"]:focus),
  :global(.code-codemirror-host .cm-panel.cm-search input:not([type]):focus) {
    border-color: rgb(var(--color-primary-400) / 0.55);
  }

  :global(.code-codemirror-host .cm-panel.cm-search button) {
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 0.3rem;
    background: rgb(var(--color-surface-800) / 0.8);
    padding: 0.15rem 0.45rem;
    color: rgb(var(--color-surface-200));
    cursor: pointer;
  }

  :global(.code-codemirror-host .cm-panel.cm-search button:hover) {
    background: rgb(var(--color-surface-700));
    color: rgb(var(--color-surface-50));
  }

  :global(.code-codemirror-host .cm-panel.cm-search label) {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    color: rgb(var(--color-surface-400));
  }

  :global(.code-codemirror-host .cm-panel.cm-search .cm-textfield) {
    min-width: 8rem;
  }

  :global(.code-codemirror-host .cm-panel.cm-search [name="close"]) {
    margin-left: auto;
  }

  /* Quiet confidence: match hits read as marks, not alarms */
  :global(.code-codemirror-host .cm-searchMatch) {
    outline: 1px solid rgb(var(--color-warning-400) / 0.35);
  }

  :global(.code-codemirror-host .cm-panel.cm-search .cm-button) {
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 0.3rem;
    background: rgb(var(--color-surface-800) / 0.8);
    color: rgb(var(--color-surface-200));
  }
</style>
