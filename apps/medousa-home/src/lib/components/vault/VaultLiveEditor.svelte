<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Editor } from "@tiptap/core";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import {
    parseLiveMarkdown,
    serializeLiveMarkdown,
    significantLiveText,
  } from "$lib/vault/live/liveMarkdownCodec";
  import { createLiveExtensions } from "$lib/vault/live/liveExtensions";
  import {
    applyLiveSlashBlock,
    clearLiveSlash,
    liveSlashOpen,
    liveSlashPrefix,
  } from "$lib/vault/live/liveSlashCommands";
  import { handleLiveHeadingKey } from "$lib/vault/live/headingKeymap";
  import { handleLiveNavKey } from "$lib/vault/live/liveNavKeymap";
  import {
    findLiquidFenceIndex,
    findViewFenceIndex,
    resolveLiveChartIndex,
  } from "$lib/vault/live/liveFenceLookup";
  import {
    isLiquidConfigureLang,
    type LiquidFenceLang,
  } from "$lib/utils/vaultLiquidFence";
  import {
    foreignUndoArmed,
    takeForeignUndo,
  } from "$lib/vault/live/liveForeignUndo";
  import { flushLiveDrafts } from "$lib/vault/live/liveDraftFlush";
  import { invalidateTransclusionCache } from "$lib/utils/resolveTransclusion";
  import { copyTextToClipboard, readTextFromClipboard } from "$lib/utils/vaultClipboard";
  import type { MarkdownFormatAction, SlashBlockId } from "$lib/utils/vaultMarkdownEdit";
  import type { MarkdownColorToken } from "$lib/utils/vaultMarkdownColors";
  import type { MarkdownFontFamily } from "$lib/utils/vaultMarkdownFonts";
  import { placeSlashMenuAnchor } from "$lib/utils/slashMenuPlacement";
  import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";
  import VaultSelectionFormatBubble from "./VaultSelectionFormatBubble.svelte";
  import VaultLiveTableChrome from "./VaultLiveTableChrome.svelte";
  import VaultLiveProperties from "./VaultLiveProperties.svelte";
  import {
    applyLiveFontFamily,
    applyLiveFontSize,
    applyLiveFormatAction,
    applyLiveTextColor,
    liveActiveFormatActions,
    withPinnedLiveScroll,
    liveCoordsAnchor,
    liveSelectionAnchor,
    liveSelectionHasText,
    liveTableChromeOpen,
    type SelectionAnchor,
  } from "$lib/vault/live/liveSelectionFormat";
  import { handleLiveScrollToSelection } from "$lib/vault/live/liveScrollSelection";
  import { toast } from "$lib/stores/toast.svelte";
  import {
    dataTransferHasImage,
    imageFileFromDataTransfer,
    markdownFromImageFile,
  } from "$lib/utils/vaultImagePaste";

  interface Props {
    /** Full note markdown (source of truth from parent). */
    value: string;
    /** Document identity — remount parent with {#key} on change; also gates reloads. */
    contentSyncKey: string;
    /** Header title — used to hide a matching leading H1 in Live (display-only). */
    displayTitle?: string;
    disabled?: boolean;
    slashOpen?: boolean;
    onchange: (next: string) => void;
    onSlashCheck?: () => void;
    onSlashKey?: (key: string) => boolean;
  }

  let {
    value,
    contentSyncKey,
    displayTitle = "",
    disabled = false,
    slashOpen = false,
    onchange,
    onSlashCheck,
    onSlashKey,
  }: Props = $props();

  let hostEl = $state<HTMLElement | null>(null);
  let editor: Editor | null = null;
  let frontmatter = $state<string | null>(null);
  let tags = $state<string[]>([]);
  let formatBubbleOpen = $state(false);
  let formatBubbleAnchor = $state<SelectionAnchor | null>(null);
  let formatActiveActions = $state<MarkdownFormatAction[]>([]);
  let formatActiveFontFamily = $state<MarkdownFontFamily | null>(null);
  let formatActiveFontSize = $state<string | null>(null);
  let formatActiveColor = $state<string | null>(null);
  /** Last nonempty selection — restored when bubble buttons steal focus. */
  let formatSelectionRange = $state<{ from: number; to: number } | null>(null);
  let removeFormatBubbleListeners: (() => void) | null = null;
  let tableChromeOpen = $state(false);
  let tableChromeAnchor = $state<SelectionAnchor | null>(null);
  let liveEditor = $state<Editor | null>(null);
  /** Key this editor instance is bound to — never flush if it diverges. */
  let boundKey = "";
  let applyingExternal = false;
  let ready = false;

  const onchangeRef = { current: onchange };
  const onSlashCheckRef = { current: onSlashCheck };
  const onSlashKeyRef = { current: onSlashKey };
  const slashOpenRef = { current: slashOpen };
  const disabledRef = { current: disabled };
  const boundKeyRef = { current: "" };
  const valueRef = { current: value };

  $effect(() => {
    onchangeRef.current = onchange;
    onSlashCheckRef.current = onSlashCheck;
    onSlashKeyRef.current = onSlashKey;
    slashOpenRef.current = slashOpen;
    disabledRef.current = disabled;
    valueRef.current = value;
  });

  function handleImageTransferEvent(
    event: ClipboardEvent | DragEvent,
    data: DataTransfer | null,
  ): boolean {
    if (disabledRef.current || !editor || !dataTransferHasImage(data)) return false;
    // Must capture File during the event — DataTransfer is cleared afterward.
    const file = imageFileFromDataTransfer(data);
    if (!file) return false;
    event.preventDefault();
    void (async () => {
      const result = await markdownFromImageFile(file);
      if (result.ok === false) {
        toast.show(result.message);
        return;
      }
      // setImage keeps the data URL intact (markdown insert can mangle huge srcs).
      editor
        ?.chain()
        .focus(undefined, { scrollIntoView: false })
        .setImage({ src: result.dataUrl, alt: result.alt })
        .run();
      emitMarkdown();
    })();
    return true;
  }

  function liveMarkdownEqual(a: string, b: string): boolean {
    const norm = (s: string) => s.replace(/\r\n/g, "\n").replace(/\n+$/g, "\n");
    return norm(a) === norm(b);
  }

  function emitMarkdown() {
    if (!editor || applyingExternal || !ready) return;
    if (boundKeyRef.current !== contentSyncKey) return;
    try {
      // While a slash prefix is active, skip full-doc serialize — filter keystrokes
      // only need TipTap's local prefix (syncSlash). Serialize resumes on close.
      if (liveSlashOpen(editor)) {
        syncSlash();
        return;
      }
      const md = serializeLiveMarkdown(editor.getJSON(), frontmatter);
      // Open/mount round-trips must not look like user edits.
      if (liveMarkdownEqual(md, valueRef.current)) return;
      onchangeRef.current(md);
    } catch (err) {
      console.error("Live emitMarkdown failed", err);
    }
  }

  /** Compare titles ignoring emoji/punctuation prefixes and whitespace. */
  function normalizeTitle(value: string): string {
    return value
      .replace(/\s+/g, " ")
      .trim()
      .replace(/^[\p{Extended_Pictographic}\p{Emoji_Presentation}\p{So}\s]+/u, "")
      .replace(/[^\p{L}\p{N}\s]+/gu, " ")
      .replace(/\s+/g, " ")
      .trim()
      .toLowerCase();
  }

  function syncDupTitleHeading() {
    if (!hostEl) return;
    const h1 = hostEl.querySelector(".ProseMirror > h1:first-child");
    if (!(h1 instanceof HTMLElement)) return;
    // Skip contenteditable=false widgets (heading `#` marks, chips). Those
    // pollute textContent and flip vault-live-h1--dup-title, which pulls the
    // H1 out of flow and shifts the whole note on the type path.
    let text = "";
    const walk = (node: Node) => {
      if (node instanceof HTMLElement && node.contentEditable === "false") return;
      if (node.nodeType === Node.TEXT_NODE) {
        text += node.textContent ?? "";
        return;
      }
      node.childNodes.forEach(walk);
    };
    walk(h1);
    const match =
      Boolean(displayTitle.trim()) &&
      normalizeTitle(text) === normalizeTitle(displayTitle);
    // Only mutate the class when the match actually changes — toggling the
    // clipped H1 in/out of flow reflows the whole note and fights scroll.
    if (h1.classList.contains("vault-live-h1--dup-title") === match) return;
    h1.classList.toggle("vault-live-h1--dup-title", match);
  }

  /** Prefer caret outside headings so open doesn't greet with `#` graffiti. */
  function placeRestingCaret() {
    if (!editor) return;
    const { doc } = editor.state;
    let target: number | null = null;
    doc.forEach((node, offset) => {
      if (target != null) return;
      if (node.type.name === "heading") return;
      if (node.isTextblock) {
        target = offset + 1;
      }
    });
    if (target == null) {
      // Fall through first heading into the next textblock if any.
      let afterHeading: number | null = null;
      let sawHeading = false;
      doc.forEach((node, offset) => {
        if (afterHeading != null) return;
        if (node.type.name === "heading") {
          sawHeading = true;
          return;
        }
        if (sawHeading && node.isTextblock) {
          afterHeading = offset + 1;
        }
      });
      target = afterHeading ?? doc.content.size;
    }
    const safe = Math.max(1, Math.min(target, doc.content.size));
    editor.commands.setTextSelection(safe);
  }

  function loadFromMarkdown(md: string) {
    if (!editor) return;
    applyingExternal = true;
    ready = false;
    try {
      const parsed = parseLiveMarkdown(md);
      frontmatter = parsed.frontmatter;
      tags = parsed.tags;
      editor.commands.setContent(parsed.doc, { contentType: "json" });
    } catch (err) {
      // A thrown setContent/parse must not leave Live permanently muted
      // (ready=false + applyingExternal=true freezes emits / slash sync).
      console.error("Live loadFromMarkdown failed", err);
      ready = true;
      applyingExternal = false;
      return;
    }
    // Keep suppress until after TipTap's deferred update notifications.
    queueMicrotask(() => {
      try {
        if (editor && !editor.isDestroyed) {
          placeRestingCaret();
          syncDupTitleHeading();
        }
      } catch (err) {
        console.error("Live resting caret failed", err);
      } finally {
        ready = true;
        applyingExternal = false;
      }
    });
  }

  function syncSlash() {
    try {
      onSlashCheckRef.current?.();
    } catch (err) {
      console.error("Live slash sync failed", err);
    }
  }

  function slashAnchorFor(container: HTMLElement | null) {
    if (!editor || !container) return null;
    // coordsAtPos can throw near atom node views — never kill the update cycle.
    const coords = liveCoordsAnchor(editor);
    if (!coords) return null;
    return placeSlashMenuAnchor(
      { top: coords.top, bottom: coords.top + coords.height, left: coords.left },
      container,
    );
  }

  function resolveContext() {
    return {
      sourcePath: vault.selectedPath,
      notes: vault.notes,
      selectedPath: vault.selectedPath,
      selectedContent: valueRef.current || vault.content,
      labelByPath: vault.labelByPath(),
    };
  }

  function liquidContext() {
    return {
      titleByPath: vault.labelByPath(),
      openLinksInWeb: false,
      localImagePath: vault.selectedPath,
      onOpenCardDetail: (detail: CardDetailPayload) => {
        vault.openCardDetail(detail);
      },
    };
  }

  function syncTableChrome() {
    if (!editor || disabled || !liveTableChromeOpen(editor)) {
      tableChromeOpen = false;
      tableChromeAnchor = null;
      return;
    }
    tableChromeAnchor = liveCoordsAnchor(editor);
    tableChromeOpen = Boolean(tableChromeAnchor);
  }

  function refreshFormatSelectionRange(
    range: { from: number; to: number } | null,
  ) {
    if (!editor) return;
    if (liveSelectionHasText(editor)) {
      const { from, to } = editor.state.selection;
      formatSelectionRange = { from, to };
    } else if (range) {
      formatSelectionRange = range;
    }
  }

  function syncFormatBubble() {
    if (!editor || disabled) {
      formatBubbleOpen = false;
      formatBubbleAnchor = null;
      formatActiveActions = [];
      formatActiveFontFamily = null;
      formatActiveFontSize = null;
      formatActiveColor = null;
      formatSelectionRange = null;
      syncTableChrome();
      return;
    }
    syncTableChrome();
    if (!liveSelectionHasText(editor)) {
      // Editor blur (e.g. before mousedown preventDefault) can empty selection —
      // keep the bubble + stashed range so the click can still apply.
      if (formatBubbleOpen && formatSelectionRange && !editor.isFocused) {
        return;
      }
      formatBubbleOpen = false;
      formatBubbleAnchor = null;
      formatActiveActions = [];
      formatActiveFontFamily = null;
      formatActiveFontSize = null;
      formatActiveColor = null;
      if (editor.isFocused) formatSelectionRange = null;
      return;
    }
    const { from, to } = editor.state.selection;
    formatSelectionRange = { from, to };
    formatBubbleAnchor = liveSelectionAnchor(editor);
    formatActiveActions = liveActiveFormatActions(editor);
    const font = editor.getAttributes("fontFamily").font as string | undefined;
    formatActiveFontFamily =
      font === "sans" || font === "serif" || font === "mono" ? font : null;
    formatActiveFontSize =
      (editor.getAttributes("fontSize").size as string | undefined) ?? null;
    formatActiveColor =
      (editor.getAttributes("textColor").color as string | undefined) ?? null;
    formatBubbleOpen = Boolean(formatBubbleAnchor);
  }

  function handleFormatAction(action: MarkdownFormatAction) {
    if (!editor) return;
    const ed = editor;
    const range = formatSelectionRange;
    withPinnedLiveScroll(ed, () => {
      applyLiveFormatAction(ed, action, range);
      refreshFormatSelectionRange(range);
      syncFormatBubble();
    });
  }

  function handleFormatColor(color: MarkdownColorToken) {
    if (!editor) return;
    const ed = editor;
    const range = formatSelectionRange;
    withPinnedLiveScroll(ed, () => {
      applyLiveTextColor(ed, color, range);
      refreshFormatSelectionRange(range);
      syncFormatBubble();
    });
  }

  function handleFormatFontFamily(font: MarkdownFontFamily) {
    if (!editor) return;
    const ed = editor;
    const range = formatSelectionRange;
    withPinnedLiveScroll(ed, () => {
      applyLiveFontFamily(ed, font, range);
      refreshFormatSelectionRange(range);
      syncFormatBubble();
    });
  }

  function handleFormatFontSize(size: string) {
    if (!editor) return;
    const ed = editor;
    const range = formatSelectionRange;
    withPinnedLiveScroll(ed, () => {
      applyLiveFontSize(ed, size, range);
      refreshFormatSelectionRange(range);
      syncFormatBubble();
    });
  }

  function commitFrontmatter(next: string | null) {
    frontmatter = next && next.trim() ? next : null;
    if (!editor) return;
    const md = serializeLiveMarkdown(editor.getJSON(), frontmatter);
    const parsed = parseLiveMarkdown(md);
    tags = parsed.tags;
    onchangeRef.current(md);
  }

  function detachEmbed(path: string, label: string, pos: number) {
    if (!editor) return;
    const token = path.replace(/\.md$/i, "");
    const href = `wikilink:${encodeURIComponent(token)}`;
    const text = label.trim() || token;
    editor
      .chain()
      .focus(undefined, { scrollIntoView: false })
      .command(({ tr, dispatch }) => {
        if (dispatch) {
          tr.replaceWith(
            pos,
            pos + 1,
            editor!.schema.text(text, [editor!.schema.marks.link.create({ href })]),
          );
        }
        return true;
      })
      .run();
  }

  async function undoForeignWriteThrough(): Promise<boolean> {
    if (!foreignUndoArmed()) return false;
    const entry = takeForeignUndo();
    if (!entry) return false;
    try {
      if (entry.path === vault.selectedPath) {
        onchangeRef.current(entry.content);
        loadFromMarkdown(entry.content);
      } else {
        await vault.saveNoteAtPath(entry.path, entry.content, { force: true });
        invalidateTransclusionCache(entry.path);
      }
      return true;
    } catch {
      return false;
    }
  }

  function handleHostClick(event: MouseEvent) {
    const target = event.target as HTMLElement;

    const configureChart = target.closest(
      ".liquid-chart-configure, [data-live-chart-configure]",
    );
    if (configureChart) {
      event.preventDefault();
      event.stopPropagation();
      const shell = configureChart.closest("[data-edit-chart-index]");
      const host = configureChart.closest<HTMLElement>("[data-live-fence-raw]");
      const raw = host?.dataset.liveFenceRaw ?? "";
      const localRaw = shell?.getAttribute("data-edit-chart-index");
      const localIndex = localRaw == null ? 0 : Number(localRaw);
      const index = resolveLiveChartIndex(
        valueRef.current || vault.content,
        raw,
        Number.isFinite(localIndex) ? localIndex : 0,
      );
      if (index >= 0) vault.openChartBridgeEdit(index);
      return;
    }

    const configureLiquid = target.closest("[data-live-liquid-configure]");
    if (configureLiquid) {
      event.preventDefault();
      event.stopPropagation();
      const langRaw =
        configureLiquid.getAttribute("data-live-liquid-lang") ?? "";
      if (!isLiquidConfigureLang(langRaw)) return;
      const lang = langRaw as LiquidFenceLang;
      const host = configureLiquid.closest<HTMLElement>("[data-live-fence-raw]");
      const raw = host?.dataset.liveFenceRaw ?? "";
      const index = findLiquidFenceIndex(
        valueRef.current || vault.content,
        lang,
        raw,
      );
      if (index >= 0) vault.openLiquidBridgeEdit(lang, index);
      return;
    }

    const configureView = target.closest(
      ".medousa-view-configure, [data-edit-view-index]",
    );
    if (configureView) {
      event.preventDefault();
      event.stopPropagation();
      const host = configureView.closest<HTMLElement>("[data-live-fence-raw]");
      const raw = host?.dataset.liveFenceRaw ?? "";
      const index = findViewFenceIndex(valueRef.current || vault.content, raw);
      if (index >= 0) vault.openViewBridgeEdit(index);
      return;
    }

    const openSource = target.closest("[data-open-vault-note]");
    if (openSource) {
      event.preventDefault();
      event.stopPropagation();
      const path = openSource.getAttribute("data-open-vault-note");
      if (path) void vault.openNote(path);
      return;
    }

    const copyCsv = target.closest("[data-copy-view-csv]");
    if (copyCsv) {
      event.preventDefault();
      event.stopPropagation();
      const payload =
        copyCsv.getAttribute("data-view-csv") ??
        copyCsv.getAttribute("data-copy-view-csv");
      if (payload) {
        try {
          void copyTextToClipboard(decodeURIComponent(payload));
        } catch {
          // ignore
        }
      }
      return;
    }

    const link = target.closest("a");
    if (link && hostEl?.contains(link)) {
      const href = link.getAttribute("href") ?? "";
      if (href.startsWith("wikilink:")) {
        event.preventDefault();
        event.stopPropagation();
        const raw = decodeURIComponent(href.slice("wikilink:".length));
        vault.openWikilink(raw);
        return;
      }
      if (href.startsWith("http://") || href.startsWith("https://")) {
        // Allow default / browser handling.
        return;
      }
    }
  }

  onMount(() => {
    if (!hostEl) return;
    const initial = value;
    const parsed = parseLiveMarkdown(initial);
    frontmatter = parsed.frontmatter;
    tags = parsed.tags;
    boundKey = contentSyncKey;
    boundKeyRef.current = contentSyncKey;
    applyingExternal = true;
    ready = false;

    editor = new Editor({
      element: hostEl,
      extensions: createLiveExtensions({
        fence: {
          getLiquidContext: liquidContext,
          getResolveContext: resolveContext,
        },
        embed: {
          getLiquidContext: liquidContext,
          getResolveContext: resolveContext,
          onOpenNote: (path) => {
            void vault.openNote(path);
          },
          onDetach: detachEmbed,
          onWriteThroughSelected: (_path, content) => {
            onchangeRef.current(content);
            loadFromMarkdown(content);
          },
          onWriteThroughForeign: async (path, content) => {
            await vault.saveNoteAtPath(path, content);
            invalidateTransclusionCache(path);
          },
        },
      }),
      content: parsed.doc,
      contentType: "json",
      editable: !disabled,
      editorProps: {
        attributes: {
          class: "vault-live-prose",
        },
        handleScrollToSelection: (view) => handleLiveScrollToSelection(view),
        handlePaste: (_view, event) =>
          handleImageTransferEvent(event, event.clipboardData),
        handleDrop: (_view, event) =>
          handleImageTransferEvent(event, event.dataTransfer),
        handleDOMEvents: {
          click: (_view, event) => {
            handleHostClick(event);
            return false;
          },
          dragover: (_view, event) => {
            if (disabledRef.current) return false;
            if (event.dataTransfer?.types.includes("Files")) {
              event.preventDefault();
              return true;
            }
            return false;
          },
        },
        handleKeyDown: (_view, event) => {
          const mod = event.metaKey || event.ctrlKey;
          if (mod && !event.altKey && !event.shiftKey && event.key.toLowerCase() === "z") {
            if (foreignUndoArmed()) {
              event.preventDefault();
              void undoForeignWriteThrough();
              return true;
            }
          }

          // Never steal ↑↓/Enter from Windows IME composition (WebView2 deadlock).
          if (
            slashOpenRef.current &&
            !event.isComposing &&
            event.keyCode !== 229
          ) {
            if (
              event.key === "ArrowDown" ||
              event.key === "ArrowUp" ||
              event.key === "Enter" ||
              event.key === "Tab" ||
              event.key === "Escape"
            ) {
              if (onSlashKeyRef.current?.(event.key)) {
                event.preventDefault();
                return true;
              }
            }
          }

          if (event.key === "Escape" && formatBubbleOpen) {
            event.preventDefault();
            formatBubbleOpen = false;
            return true;
          }

          if (editor && handleLiveNavKey(editor, event)) {
            return true;
          }

          if (editor && handleLiveHeadingKey(editor, event)) {
            return true;
          }

          if (mod && !event.altKey && !event.shiftKey && editor) {
            const key = event.key.toLowerCase();
            if (key === "b") {
              event.preventDefault();
              editor.chain().focus(undefined, { scrollIntoView: false }).toggleBold().run();
              return true;
            }
            if (key === "i") {
              event.preventDefault();
              editor.chain().focus(undefined, { scrollIntoView: false }).toggleItalic().run();
              return true;
            }
            if (key === "e") {
              event.preventDefault();
              editor.chain().focus(undefined, { scrollIntoView: false }).toggleCode().run();
              return true;
            }
            if (key === "k") {
              event.preventDefault();
              const prev = editor.getAttributes("link").href as string | undefined;
              const href = window.prompt("Link URL", prev ?? "https://");
              if (href === null) return true;
              if (!href) {
                editor.chain().focus(undefined, { scrollIntoView: false }).unsetLink().run();
              } else {
                editor
                  .chain()
                  .focus(undefined, { scrollIntoView: false })
                  .extendMarkRange("link")
                  .setLink({ href })
                  .run();
              }
              return true;
            }
            if (key === "f") {
              event.preventDefault();
              vaultFind.setSourceText(
                serializeLiveMarkdown(editor.getJSON(), frontmatter),
              );
              vaultFind.openFind();
              return true;
            }
          }
          return false;
        },
      },
      onUpdate: ({ transaction }) => {
        // Ignore selection-only / history-less programmatic loads.
        if (!transaction.docChanged) return;
        if (transaction.getMeta("addToHistory") === false) return;
        emitMarkdown();
        syncSlash();
        // Guarded: only mutates the class when the H1↔title match flips.
        syncDupTitleHeading();
      },
      onSelectionUpdate: () => {
        syncSlash();
        syncFormatBubble();
      },
      onBlur: () => {
        queueMicrotask(() => {
          const active = document.activeElement;
          if (active?.closest(".vault-selection-format-bubble")) return;
          if (active?.closest(".vault-live-table-chrome")) return;
          if (editor && liveSelectionHasText(editor)) {
            syncFormatBubble();
            return;
          }
          formatBubbleOpen = false;
          formatSelectionRange = null;
          syncTableChrome();
        });
      },
    });

    liveEditor = editor;

    const scrollParent = hostEl.closest(".vault-live-editor");
    const onScrollOrResize = () => syncFormatBubble();
    scrollParent?.addEventListener("scroll", onScrollOrResize, { passive: true });
    window.addEventListener("resize", onScrollOrResize);
    removeFormatBubbleListeners = () => {
      scrollParent?.removeEventListener("scroll", onScrollOrResize);
      window.removeEventListener("resize", onScrollOrResize);
    };

    // Defer accepting emits until after mount + node-view setup settles.
    queueMicrotask(() => {
      if (value !== initial) {
        loadFromMarkdown(value);
        return;
      }
      requestAnimationFrame(() => {
        placeRestingCaret();
        ready = true;
        applyingExternal = false;
        syncDupTitleHeading();
      });
    });
  });

  $effect(() => {
    if (!editor) return;
    editor.setEditable(!disabled);
  });

  $effect(() => {
    displayTitle;
    syncDupTitleHeading();
  });

  /** Cap same-key empty→content reloads so cut/paste races cannot freeze the app. */
  let emptyHydrateAttempts = 0;
  let emptyHydrateKey = "";

  /**
   * Parent should remount this component with `{#key contentSyncKey}` on note switch.
   * Same-key path only hydrates empty→content when the body has significant text
   * (mount race). Frontmatter-only notes must not re-enter loadFromMarkdown or
   * TipTap stays `isEmpty` forever and `$effect` loops. Never re-parse on typing.
   */
  $effect(() => {
    if (!editor || applyingExternal) return;
    const key = contentSyncKey;
    const next = value;
    try {
      if (key !== boundKey) {
        boundKey = key;
        boundKeyRef.current = key;
        emptyHydrateKey = key;
        emptyHydrateAttempts = 0;
        loadFromMarkdown(next);
        return;
      }
      // Mount race: editor came up empty before draft hydrated. Only reload when
      // there is significant body text TipTap would actually render.
      if (editor.isEmpty && significantLiveText(next).length > 0) {
        if (emptyHydrateKey !== key) {
          emptyHydrateKey = key;
          emptyHydrateAttempts = 0;
        }
        if (emptyHydrateAttempts >= 2) return;
        emptyHydrateAttempts += 1;
        loadFromMarkdown(next);
      }
    } catch (err) {
      console.error("Live hydrate effect failed", err);
      ready = true;
      applyingExternal = false;
    }
  });

  onDestroy(() => {
    // Do NOT serialize→onchange here. Leave-flush / explicit flush() own that
    // handoff; destroy flushes race note switches and clobber the leased path.
    removeFormatBubbleListeners?.();
    removeFormatBubbleListeners = null;
    editor?.destroy();
    editor = null;
    liveEditor = null;
    tableChromeOpen = false;
    tableChromeAnchor = null;
  });

  /** Explicit serialize for Live→Build plane switch (caller must invoke before unmount). */
  export function flush(): string {
    // Promote nested Write drafts (slides/report inputs) into TipTap attrs first.
    flushLiveDrafts();
    if (!editor) return value;
    return serializeLiveMarkdown(editor.getJSON(), frontmatter);
  }

  export function getSlashAnchor(container: HTMLElement | null) {
    return slashAnchorFor(container);
  }

  export function isSlashOpen(): boolean {
    return editor ? liveSlashOpen(editor) : false;
  }

  export function slashFilter(): string {
    return editor ? (liveSlashPrefix(editor) ?? "") : "";
  }

  export function applySlash(block: SlashBlockId): boolean {
    if (!editor) return false;
    return applyLiveSlashBlock(editor, block);
  }

  export function clearSlash(): boolean {
    if (!editor) return false;
    return clearLiveSlash(editor);
  }

  export function insertText(text: string): void {
    if (!editor) return;
    editor.chain().focus(undefined, { scrollIntoView: false }).insertContent(text).run();
  }

  export function insertFence(raw: string): void {
    if (!editor) return;
    editor
      .chain()
      .focus(undefined, { scrollIntoView: false })
      .insertFenceBlock(raw.trimEnd() + "\n")
      .run();
  }

  export function insertEmbed(path: string, label?: string): void {
    if (!editor) return;
    editor
      .chain()
      .focus(undefined, { scrollIntoView: false })
      .insertEmbedBlock(path, label)
      .run();
  }

  export function insertWikilink(path: string, label: string): void {
    if (!editor) return;
    const token = path.replace(/\.md$/i, "");
    const href = `wikilink:${encodeURIComponent(token)}`;
    const text = label.trim() || token;
    editor
      .chain()
      .focus(undefined, { scrollIntoView: false })
      .insertContent({
        type: "text",
        text,
        marks: [{ type: "link", attrs: { href } }],
      })
      .run();
  }

  export function focus() {
    editor?.commands.focus(undefined, { scrollIntoView: false });
  }

  export function getScrollEl(): HTMLElement | null {
    return hostEl?.closest(".vault-live-editor") as HTMLElement | null;
  }

  export function getSelectedText(): string {
    if (!editor) return "";
    const { from, to } = editor.state.selection;
    if (from === to) return "";
    return editor.state.doc.textBetween(from, to, "\n");
  }

  export function hasTextSelection(): boolean {
    return editor ? liveSelectionHasText(editor) : false;
  }

  export async function copySelection(): Promise<boolean> {
    const text = getSelectedText();
    if (!text.trim()) return false;
    await copyTextToClipboard(text);
    return true;
  }

  export async function cutSelection(): Promise<boolean> {
    if (!editor || disabled) return false;
    const ok = await copySelection();
    if (!ok) return false;
    editor.chain().focus(undefined, { scrollIntoView: false }).deleteSelection().run();
    return true;
  }

  export async function pasteClipboard(): Promise<boolean> {
    if (!editor || disabled) return false;
    const text = await readTextFromClipboard();
    if (!text) {
      toast.show("Couldn’t read clipboard — try Ctrl+V / ⌘V", { durationMs: 2800 });
      return false;
    }
    editor.chain().focus(undefined, { scrollIntoView: false }).insertContent(text).run();
    return true;
  }

  export function selectAll() {
    editor?.chain().focus(undefined, { scrollIntoView: false }).selectAll().run();
  }

  export function applyFormat(action: MarkdownFormatAction) {
    if (!editor || disabled) return;
    applyLiveFormatAction(editor, action);
  }
</script>

<div class="vault-live-editor flex min-h-0 min-w-0 max-w-full flex-1 flex-col overflow-x-hidden overflow-y-auto">
  <VaultLiveProperties
    {frontmatter}
    {tags}
    fallbackTitle={displayTitle}
    disabled={disabled}
    onFrontmatterChange={commitFrontmatter}
  />
  <div bind:this={hostEl} class="vault-live-editor__host min-h-0 flex-1"></div>
</div>

<style>
  :global(.vault-live-prose img.vault-live-image) {
    display: block;
    max-width: 100%;
    height: auto;
    margin: 0.75rem 0;
    border-radius: 0.35rem;
  }
</style>

<VaultSelectionFormatBubble
  open={formatBubbleOpen}
  anchor={formatBubbleAnchor}
  activeActions={formatActiveActions}
  activeFontFamily={formatActiveFontFamily}
  activeFontSize={formatActiveFontSize}
  activeColor={formatActiveColor}
  disabled={disabled}
  onFormat={handleFormatAction}
  onColor={handleFormatColor}
  onFontFamily={handleFormatFontFamily}
  onFontSize={handleFormatFontSize}
  onClose={() => {
    formatBubbleOpen = false;
  }}
/>

<VaultLiveTableChrome
  open={tableChromeOpen && !formatBubbleOpen}
  anchor={tableChromeAnchor}
  editor={liveEditor}
  disabled={disabled}
/>
