<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { basicSetup } from "codemirror";
  import { EditorState, Compartment, Prec } from "@codemirror/state";
  import { EditorView, keymap, placeholder } from "@codemirror/view";
  import { defaultKeymap, historyKeymap } from "@codemirror/commands";
  import { markdown } from "@codemirror/lang-markdown";
  import {
    activeMarkdownFormats,
    applyMarkdownColor,
    applyMarkdownFormat,
    backspaceListPrefix,
    continueListOnEnter,
    findHeadingSourceOffset,
    indentLines,
    outdentLines,
    type EditResult,
    type MarkdownColorToken,
    type MarkdownFormatAction,
  } from "$lib/utils/vaultMarkdownEdit";
  import {
    applyEditResult,
    getCodeMirrorCaretAnchor,
    revealFindMatchInView,
    vaultEditorBaseTheme,
    vaultFindHighlightExtension,
  } from "$lib/utils/vaultCodeMirror";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import type { FindMatch } from "$lib/utils/vaultFindInNote";

  interface Props {
    value: string;
    contentSyncKey: string;
    disabled?: boolean;
    surface?: "write" | "source";
    class?: string;
    onchange?: (value: string) => void;
    onSelectionChange?: (start: number, end: number) => void;
    onSlashCheck?: () => void;
  }

  let {
    value,
    contentSyncKey,
    disabled = false,
    surface = "write",
    class: className = "",
    onchange,
    onSelectionChange,
    onSlashCheck,
  }: Props = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined = $state();
  let syncedKey = $state("");
  const readOnlyCompartment = new Compartment();
  let findDecorationsEpoch = $state(0);

  let findMatches = $state<FindMatch[]>([]);
  let findActiveIndex = $state(0);

  function emitSelection() {
    if (!view) return;
    const sel = view.state.selection.main;
    onSelectionChange?.(sel.from, sel.to);
  }

  function applyExternalEdit(result: EditResult) {
    if (!view) return;
    applyEditResult(view, result);
    onchange?.(view.state.doc.toString());
    emitSelection();
    onSlashCheck?.();
  }

  export function applyEdit(result: EditResult) {
    applyExternalEdit(result);
  }

  export function getContent(): string {
    return view?.state.doc.toString() ?? value;
  }

  export function getSelection(): { start: number; end: number } {
    if (!view) return { start: 0, end: 0 };
    const sel = view.state.selection.main;
    return { start: sel.from, end: sel.to };
  }

  export function getScrollEl(): HTMLElement | null {
    return view?.scrollDOM ?? null;
  }

  export function getView(): EditorView | undefined {
    return view;
  }

  export function focusEditor() {
    view?.focus();
  }

  export function getActiveFormats(): MarkdownFormatAction[] {
    if (!view) return [];
    const sel = view.state.selection.main;
    return activeMarkdownFormats(view.state.doc.toString(), sel.from, sel.to);
  }

  export function getSlashAnchor(relativeTo?: HTMLElement | null) {
    if (!view) return { top: 40, left: 16 };
    return getCodeMirrorCaretAnchor(view, relativeTo);
  }

  export function format(action: MarkdownFormatAction) {
    if (!view) return;
    const content = view.state.doc.toString();
    const sel = view.state.selection.main;
    applyExternalEdit(applyMarkdownFormat(content, sel.from, sel.to, action));
  }

  export function color(token: MarkdownColorToken) {
    if (!view) return;
    const content = view.state.doc.toString();
    const sel = view.state.selection.main;
    applyExternalEdit(applyMarkdownColor(content, sel.from, sel.to, token));
  }

  export function scrollToHeadingSource(headingText: string) {
    if (!view) return;
    const content = view.state.doc.toString();
    const offset = findHeadingSourceOffset(content, headingText);
    if (offset == null) return;
    const line = view.state.doc.lineAt(offset);
    view.dispatch({
      selection: { anchor: line.from, head: line.to },
      effects: EditorView.scrollIntoView(line.from, { y: "start", yMargin: 40 }),
    });
    view.focus();
  }

  export function refreshFindHighlights(matches: FindMatch[], activeIndex: number) {
    findMatches = matches;
    findActiveIndex = activeIndex;
    findDecorationsEpoch += 1;
    if (view) {
      // Force plugin refresh
      view.dispatch({});
      revealFindMatchInView(view, matches[activeIndex] ?? null);
    }
  }

  function buildKeymap() {
    return Prec.highest(
      keymap.of([
        {
          key: "Mod-b",
          run: () => {
            format("bold");
            return true;
          },
        },
        {
          key: "Mod-i",
          run: () => {
            format("italic");
            return true;
          },
        },
        {
          key: "Mod-e",
          run: () => {
            format("code");
            return true;
          },
        },
        {
          key: "Mod-k",
          run: () => {
            format("link");
            return true;
          },
        },
        {
          key: "Mod-h",
          run: () => {
            format("highlight");
            return true;
          },
        },
        {
          key: "Mod-f",
          run: () => {
            vaultFind.setSourceText(getContent());
            vaultFind.openFind();
            return true;
          },
        },
        {
          key: "Mod-Alt-f",
          run: () => {
            vaultFind.setSourceText(getContent());
            vaultFind.openReplace();
            return true;
          },
        },
        {
          key: "Tab",
          run: () => {
            if (!view) return false;
            const content = view.state.doc.toString();
            const sel = view.state.selection.main;
            applyExternalEdit(indentLines(content, sel.from, sel.to));
            return true;
          },
        },
        {
          key: "Shift-Tab",
          run: () => {
            if (!view) return false;
            const content = view.state.doc.toString();
            const sel = view.state.selection.main;
            applyExternalEdit(outdentLines(content, sel.from, sel.to));
            return true;
          },
        },
        {
          key: "Enter",
          run: () => {
            if (!view) return false;
            const sel = view.state.selection.main;
            if (sel.from !== sel.to) return false;
            const content = view.state.doc.toString();
            const result = continueListOnEnter(content, sel.from);
            if (!result) return false;
            applyExternalEdit(result);
            return true;
          },
        },
        {
          key: "Backspace",
          run: () => {
            if (!view) return false;
            const sel = view.state.selection.main;
            if (sel.from !== sel.to) return false;
            const content = view.state.doc.toString();
            const result = backspaceListPrefix(content, sel.from);
            if (!result) return false;
            applyExternalEdit(result);
            return true;
          },
        },
      ]),
    );
  }

  onMount(() => {
    if (!host) return;
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          markdown(),
          EditorView.lineWrapping,
          vaultEditorBaseTheme,
          placeholder("Write…"),
          readOnlyCompartment.of(EditorState.readOnly.of(disabled)),
          buildKeymap(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          vaultFindHighlightExtension(() => ({
            matches: findMatches,
            activeIndex: findActiveIndex,
          })),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              onchange?.(update.state.doc.toString());
              onSlashCheck?.();
            }
            if (update.selectionSet || update.docChanged) {
              emitSelection();
            }
          }),
          EditorView.domEventHandlers({
            scroll: () => {
              onSlashCheck?.();
              return false;
            },
          }),
        ],
      }),
    });
    syncedKey = contentSyncKey;
    vaultFind.registerCodeMirror(view);
  });

  onDestroy(() => {
    vaultFind.registerCodeMirror(null);
    view?.destroy();
    view = undefined;
  });

  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: readOnlyCompartment.reconfigure(EditorState.readOnly.of(disabled)),
    });
  });

  $effect(() => {
    if (!view) return;
    if (contentSyncKey === syncedKey) return;
    const next = value;
    syncedKey = contentSyncKey;
    if (view.state.doc.toString() === next) return;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: next },
    });
  });

  // Keep class in sync for write/source typography variants
  $effect(() => {
    surface;
    className;
  });
</script>

<div
  bind:this={host}
  class="vault-codemirror-host vault-codemirror-host--{surface} min-h-0 flex-1 {className}"
  data-find-epoch={findDecorationsEpoch}
></div>
