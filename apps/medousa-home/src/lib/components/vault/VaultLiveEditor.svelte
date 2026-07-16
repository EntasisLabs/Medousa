<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Editor } from "@tiptap/core";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import {
    parseLiveMarkdown,
    serializeLiveMarkdown,
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
    findViewFenceIndex,
    resolveLiveChartIndex,
  } from "$lib/vault/live/liveFenceLookup";
  import {
    foreignUndoArmed,
    takeForeignUndo,
  } from "$lib/vault/live/liveForeignUndo";
  import { saveVaultNote } from "$lib/daemon";
  import { invalidateTransclusionCache } from "$lib/utils/resolveTransclusion";
  import { copyTextToClipboard } from "$lib/utils/vaultClipboard";
  import type { SlashBlockId } from "$lib/utils/vaultMarkdownEdit";
  import {
    restingVaultTagChips,
    sortVaultTagsForDisplay,
  } from "$lib/utils/vaultFrontmatter";

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
  let tagsExpanded = $state(false);

  const tagChips = $derived(restingVaultTagChips(tags, 2));
  const shownTags = $derived(
    tagsExpanded ? sortVaultTagsForDisplay(tags) : tagChips.visible,
  );
  /** Key this editor instance is bound to — never flush if it diverges. */
  let boundKey = "";
  let applyingExternal = false;
  let ready = false;

  const onchangeRef = { current: onchange };
  const onSlashCheckRef = { current: onSlashCheck };
  const onSlashKeyRef = { current: onSlashKey };
  const slashOpenRef = { current: slashOpen };
  const boundKeyRef = { current: "" };
  const valueRef = { current: value };

  $effect(() => {
    onchangeRef.current = onchange;
    onSlashCheckRef.current = onSlashCheck;
    onSlashKeyRef.current = onSlashKey;
    slashOpenRef.current = slashOpen;
    valueRef.current = value;
  });

  function liveMarkdownEqual(a: string, b: string): boolean {
    const norm = (s: string) => s.replace(/\r\n/g, "\n").replace(/\n+$/g, "\n");
    return norm(a) === norm(b);
  }

  function emitMarkdown() {
    if (!editor || applyingExternal || !ready) return;
    if (boundKeyRef.current !== contentSyncKey) return;
    const md = serializeLiveMarkdown(editor.getJSON(), frontmatter);
    // Open/mount round-trips must not look like user edits.
    if (liveMarkdownEqual(md, valueRef.current)) return;
    onchangeRef.current(md);
  }

  function normalizeTitle(value: string): string {
    return value.replace(/\s+/g, " ").trim().toLowerCase();
  }

  function syncDupTitleHeading() {
    if (!hostEl) return;
    const h1 = hostEl.querySelector(".ProseMirror > h1:first-child");
    if (!(h1 instanceof HTMLElement)) return;
    const match =
      Boolean(displayTitle.trim()) &&
      normalizeTitle(h1.textContent ?? "") === normalizeTitle(displayTitle);
    h1.classList.toggle("vault-live-h1--dup-title", match);
  }

  function loadFromMarkdown(md: string) {
    if (!editor) return;
    const parsed = parseLiveMarkdown(md);
    frontmatter = parsed.frontmatter;
    tags = parsed.tags;
    tagsExpanded = false;
    applyingExternal = true;
    ready = false;
    editor.commands.setContent(parsed.doc, { contentType: "json" });
    // Keep suppress until after TipTap's deferred update notifications.
    queueMicrotask(() => {
      ready = true;
      applyingExternal = false;
      syncDupTitleHeading();
    });
  }

  function syncSlash() {
    onSlashCheckRef.current?.();
  }

  function slashAnchorFor(container: HTMLElement | null): { top: number; left: number } | null {
    if (!editor || !container) return null;
    const { from } = editor.state.selection;
    const coords = editor.view.coordsAtPos(from);
    const rect = container.getBoundingClientRect();
    return {
      top: coords.bottom - rect.top + 6,
      left: coords.left - rect.left,
    };
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
    };
  }

  function detachEmbed(path: string, label: string, pos: number) {
    if (!editor) return;
    const token = path.replace(/\.md$/i, "");
    const href = `wikilink:${encodeURIComponent(token)}`;
    const text = label.trim() || token;
    editor
      .chain()
      .focus()
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
        await saveVaultNote(entry.path, entry.content);
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
        },
      }),
      content: parsed.doc,
      contentType: "json",
      editable: !disabled,
      editorProps: {
        attributes: {
          class: "vault-live-prose",
        },
        handleDOMEvents: {
          click: (_view, event) => {
            handleHostClick(event);
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

          if (slashOpenRef.current) {
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
              editor.chain().focus().toggleBold().run();
              return true;
            }
            if (key === "i") {
              event.preventDefault();
              editor.chain().focus().toggleItalic().run();
              return true;
            }
            if (key === "e") {
              event.preventDefault();
              editor.chain().focus().toggleCode().run();
              return true;
            }
            if (key === "k") {
              event.preventDefault();
              const prev = editor.getAttributes("link").href as string | undefined;
              const href = window.prompt("Link URL", prev ?? "https://");
              if (href === null) return true;
              if (!href) {
                editor.chain().focus().unsetLink().run();
              } else {
                editor.chain().focus().extendMarkRange("link").setLink({ href }).run();
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
        syncDupTitleHeading();
      },
      onSelectionUpdate: () => {
        syncSlash();
      },
    });

    // Defer accepting emits until after mount + node-view setup settles.
    queueMicrotask(() => {
      if (value !== initial) {
        loadFromMarkdown(value);
        return;
      }
      requestAnimationFrame(() => {
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

  /**
   * Parent should remount this component with `{#key contentSyncKey}` on note switch.
   * Same-key path only hydrates empty→content (mount race). Never re-parse on typing.
   */
  $effect(() => {
    if (!editor) return;
    const key = contentSyncKey;
    const next = value;
    if (key !== boundKey) {
      boundKey = key;
      boundKeyRef.current = key;
      loadFromMarkdown(next);
      return;
    }
    if (next.trim() && editor.isEmpty) {
      loadFromMarkdown(next);
    }
  });

  onDestroy(() => {
    editor?.destroy();
    editor = null;
  });

  /** Explicit serialize for Live→Build plane switch (caller must invoke before unmount). */
  export function flush(): string {
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
    editor.chain().focus().insertContent(text).run();
  }

  export function insertFence(raw: string): void {
    if (!editor) return;
    editor.chain().focus().insertFenceBlock(raw.trimEnd() + "\n").run();
  }

  export function insertEmbed(path: string, label?: string): void {
    if (!editor) return;
    editor.chain().focus().insertEmbedBlock(path, label).run();
  }

  export function insertWikilink(path: string, label: string): void {
    if (!editor) return;
    const token = path.replace(/\.md$/i, "");
    const href = `wikilink:${encodeURIComponent(token)}`;
    const text = label.trim() || token;
    editor
      .chain()
      .focus()
      .insertContent({
        type: "text",
        text,
        marks: [{ type: "link", attrs: { href } }],
      })
      .run();
  }

  export function focus() {
    editor?.commands.focus();
  }

  export function getScrollEl(): HTMLElement | null {
    return hostEl?.closest(".vault-live-editor") as HTMLElement | null;
  }
</script>

<div class="vault-live-editor flex min-h-0 flex-1 flex-col overflow-y-auto">
  {#if tags.length > 0 && (shownTags.length > 0 || tagChips.hiddenCount > 0)}
    <div class="vault-live-tag-chips" aria-label="Tags">
      {#each shownTags as tag (tag)}
        <span class="vault-live-tag-chip">{tag}</span>
      {/each}
      {#if !tagsExpanded && tagChips.hiddenCount > 0}
        <button
          type="button"
          class="vault-live-tag-chip vault-live-tag-chip--more"
          aria-label="Show {tagChips.hiddenCount} more tags"
          onclick={() => (tagsExpanded = true)}
        >
          +{tagChips.hiddenCount}
        </button>
      {/if}
    </div>
  {/if}
  <div bind:this={hostEl} class="vault-live-editor__host min-h-0 flex-1"></div>
</div>
