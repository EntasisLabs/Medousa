<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { basicSetup } from "codemirror";
  import { EditorState, Compartment, Prec } from "@codemirror/state";
  import { EditorView, keymap, placeholder } from "@codemirror/view";
  import { historyKeymap } from "@codemirror/commands";
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
    vaultMarkdownSyntax,
  } from "$lib/utils/vaultCodeMirror";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import type { FindMatch } from "$lib/utils/vaultFindInNote";

  interface Props {
    value: string;
    contentSyncKey: string;
    disabled?: boolean;
    surface?: "write" | "source";
    class?: string;
    /** When true, ↑↓/Enter/Esc are claimed for the slash menu (incl. Shift). */
    slashOpen?: boolean;
    onchange?: (value: string) => void;
    onSelectionChange?: (start: number, end: number) => void;
    onSlashCheck?: () => void;
    /** Return true when the slash menu consumed the key. */
    onSlashKey?: (key: string) => boolean;
  }

  let {
    value,
    contentSyncKey,
    disabled = false,
    surface = "write",
    class: className = "",
    slashOpen = false,
    onchange,
    onSelectionChange,
    onSlashCheck,
    onSlashKey,
  }: Props = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined = $state();
  let syncedKey = $state("");
  const readOnlyCompartment = new Compartment();
  const slashKeymapCompartment = new Compartment();
  let findDecorationsEpoch = $state(0);

  let findMatches = $state<FindMatch[]>([]);
  let findActiveIndex = $state(0);

  // Mount-stable refs for keymap run functions.
  let slashOpenRef = false;
  let slashKeyHandler: ((key: string) => boolean) | null = null;
  $effect(() => {
    slashOpenRef = slashOpen;
    slashKeyHandler = onSlashKey ?? null;
  });

  function slashKeymapExt(active: boolean) {
    if (!active) return [];
    // While the menu is open, always claim these keys (return true) so
    // defaultKeymap cursor/select line bindings never run — including Shift.
    const claim = (key: string) => () => {
      slashKeyHandler?.(key);
      return true;
    };
    return Prec.highest(
      keymap.of([
        {
          key: "ArrowDown",
          run: claim("ArrowDown"),
          shift: claim("ArrowDown"),
          preventDefault: true,
        },
        {
          key: "ArrowUp",
          run: claim("ArrowUp"),
          shift: claim("ArrowUp"),
          preventDefault: true,
        },
        {
          key: "Enter",
          run: claim("Enter"),
          preventDefault: true,
        },
        {
          key: "Escape",
          run: claim("Escape"),
          preventDefault: true,
        },
      ]),
    );
  }

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

  /** Focus Build at a character offset (e.g. Live fence → source). */
  export function focusOffset(offset: number, end?: number) {
    if (!view) return;
    const doc = view.state.doc;
    const from = Math.max(0, Math.min(offset, doc.length));
    const to = Math.max(from, Math.min(end ?? from, doc.length));
    view.dispatch({
      selection: { anchor: from, head: to },
      effects: EditorView.scrollIntoView(from, { y: "center", yMargin: 48 }),
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
          key: "Enter",
          run: () => {
            // Slash Enter is handled in domEventHandlers (single path).
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
          vaultMarkdownSyntax,
          EditorView.lineWrapping,
          vaultEditorBaseTheme,
          placeholder("Write…"),
          readOnlyCompartment.of(EditorState.readOnly.of(disabled)),
          slashKeymapCompartment.of(slashKeymapExt(slashOpen)),
          buildKeymap(),
          // historyKeymap only — defaultKeymap already comes from basicSetup
          keymap.of(historyKeymap),
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
    // Parent draft often fills after mount — pull current value immediately.
    if (value && view.state.doc.toString() !== value) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
      });
    }
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
    view.dispatch({
      effects: slashKeymapCompartment.reconfigure(slashKeymapExt(slashOpen)),
    });
  });

  /**
   * Keep CM doc aligned with the parent `value`.
   * Handles the common race where we mount with "" before draft hydrates
   * (same contentSyncKey, so a key-only gate would skip forever).
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
    // External note switch, or mount-before-hydrate catch-up.
    if (key !== syncedKey || (current.length === 0 && next.length > 0)) {
      syncedKey = key;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: next },
      });
    }
  });
</script>

<div
  bind:this={host}
  class="vault-codemirror-host vault-codemirror-host--{surface} min-h-0 flex-1 {className}"
  data-find-epoch={findDecorationsEpoch}
></div>
