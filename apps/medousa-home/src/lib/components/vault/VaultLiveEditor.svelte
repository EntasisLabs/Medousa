<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Editor } from "@tiptap/core";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import {
    parseLiveMarkdown,
    serializeLiveMarkdown,
  } from "$lib/vault/live/liveMarkdownCodec";
  import { createLiveExtensions } from "$lib/vault/live/liveExtensions";
  import type { FenceBlockAttrs } from "$lib/vault/live/fenceBlockExtension";
  import {
    applyLiveSlashBlock,
    clearLiveSlash,
    liveSlashOpen,
    liveSlashPrefix,
  } from "$lib/vault/live/liveSlashCommands";
  import type { SlashBlockId } from "$lib/utils/vaultMarkdownEdit";

  interface Props {
    value: string;
    contentSyncKey: string;
    disabled?: boolean;
    slashOpen?: boolean;
    onchange: (next: string) => void;
    onSlashCheck?: () => void;
    onSlashKey?: (key: string) => boolean;
    onEditFenceInBuild?: (raw: string) => void;
  }

  let {
    value,
    contentSyncKey,
    disabled = false,
    slashOpen = false,
    onchange,
    onSlashCheck,
    onSlashKey,
    onEditFenceInBuild,
  }: Props = $props();

  let hostEl = $state<HTMLElement | null>(null);
  let editor: Editor | null = null;
  let frontmatter = $state<string | null>(null);
  let tags = $state<string[]>([]);
  let syncedKey = $state("");
  let applyingExternal = false;
  /** Ignore TipTap noise until initial setContent settles. */
  let ready = false;
  /** Last markdown we emitted or loaded — used to skip echo reloads + empty wipes. */
  let lastGoodMarkdown = "";

  const onchangeRef = { current: onchange };
  const onSlashCheckRef = { current: onSlashCheck };
  const onSlashKeyRef = { current: onSlashKey };
  const onEditFenceRef = { current: onEditFenceInBuild };
  const slashOpenRef = { current: slashOpen };

  $effect(() => {
    onchangeRef.current = onchange;
    onSlashCheckRef.current = onSlashCheck;
    onSlashKeyRef.current = onSlashKey;
    onEditFenceRef.current = onEditFenceInBuild;
    slashOpenRef.current = slashOpen;
  });

  function isEffectivelyEmpty(md: string): boolean {
    const { content } = (() => {
      // local strip to avoid importing cycles in hot paths — use same rule as codec
      const trimmed = md.trimStart();
      if (!trimmed.startsWith("---")) return { content: md };
      const rest = trimmed.slice(3);
      const end = rest.indexOf("\n---");
      if (end === -1) return { content: md };
      return { content: rest.slice(end + 4) };
    })();
    return content.replace(/\s+/g, "").length === 0;
  }

  function emitMarkdown() {
    if (!editor || applyingExternal || !ready) return;
    const md = serializeLiveMarkdown(editor.getJSON(), frontmatter);
    // Never replace a real note with an empty serialize (mount/destroy races).
    if (isEffectivelyEmpty(md) && !isEffectivelyEmpty(lastGoodMarkdown)) {
      return;
    }
    if (md === lastGoodMarkdown) return;
    lastGoodMarkdown = md;
    onchangeRef.current(md);
  }

  function loadFromMarkdown(md: string) {
    if (!editor) return;
    if (md === lastGoodMarkdown && !isEffectivelyEmpty(md)) {
      return;
    }
    const parsed = parseLiveMarkdown(md);
    frontmatter = parsed.frontmatter;
    tags = parsed.tags;
    applyingExternal = true;
    ready = false;
    editor.commands.setContent(parsed.doc, { contentType: "json" });
    lastGoodMarkdown = md;
    applyingExternal = false;
    // Allow emits on the next tick so setContent's update is ignored.
    queueMicrotask(() => {
      ready = true;
    });
  }

  function handleFenceEdit(attrs: FenceBlockAttrs) {
    onEditFenceRef.current?.(attrs.raw);
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

  onMount(() => {
    if (!hostEl) return;
    const initial = value;
    const parsed = parseLiveMarkdown(initial);
    frontmatter = parsed.frontmatter;
    tags = parsed.tags;
    lastGoodMarkdown = initial;

    editor = new Editor({
      element: hostEl,
      extensions: createLiveExtensions({
        onEditInBuild: handleFenceEdit,
      }),
      content: parsed.doc,
      contentType: "json",
      editable: !disabled,
      editorProps: {
        attributes: {
          class: "vault-live-prose",
        },
        handleKeyDown: (_view, event) => {
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

          const mod = event.metaKey || event.ctrlKey;
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
      onUpdate: () => {
        emitMarkdown();
        syncSlash();
      },
      onSelectionUpdate: () => {
        syncSlash();
      },
    });

    syncedKey = contentSyncKey;
    queueMicrotask(() => {
      ready = true;
      // Catch mount-before-hydrate: parent draft filled after TipTap created.
      if (value && value !== lastGoodMarkdown) {
        loadFromMarkdown(value);
      }
    });
  });

  $effect(() => {
    if (!editor) return;
    editor.setEditable(!disabled);
  });

  /**
   * Align with external note switches — not with our own markDirty echo.
   * Same pattern as CodeMirror: only replace when value actually differs.
   */
  $effect(() => {
    if (!editor) return;
    const next = value;
    const key = contentSyncKey;
    if (next === lastGoodMarkdown) {
      syncedKey = key;
      return;
    }
    if (key !== syncedKey || (isEffectivelyEmpty(lastGoodMarkdown) && !isEffectivelyEmpty(next))) {
      syncedKey = key;
      loadFromMarkdown(next);
    }
  });

  onDestroy(() => {
    if (!editor) return;
    // Flush only when we have real content — never wipe the open note on unmount
    // (noteLoading tears the editor down while switching notes).
    if (ready) {
      const md = serializeLiveMarkdown(editor.getJSON(), frontmatter);
      if (!isEffectivelyEmpty(md) || isEffectivelyEmpty(lastGoodMarkdown)) {
        if (md !== lastGoodMarkdown) {
          lastGoodMarkdown = md;
          onchangeRef.current(md);
        }
      }
    }
    editor.destroy();
    editor = null;
  });

  export function flush(): string {
    if (!editor) return lastGoodMarkdown || value;
    const md = serializeLiveMarkdown(editor.getJSON(), frontmatter);
    if (isEffectivelyEmpty(md) && !isEffectivelyEmpty(lastGoodMarkdown)) {
      return lastGoodMarkdown;
    }
    lastGoodMarkdown = md;
    return md;
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

  export function focus() {
    editor?.commands.focus();
  }

  export function getScrollEl(): HTMLElement | null {
    return hostEl?.closest(".vault-live-editor") as HTMLElement | null;
  }
</script>

<div class="vault-live-editor flex min-h-0 flex-1 flex-col overflow-y-auto">
  {#if tags.length > 0}
    <div class="vault-live-tag-chips" aria-label="Tags">
      {#each tags as tag (tag)}
        <span class="vault-live-tag-chip">{tag}</span>
      {/each}
    </div>
  {/if}
  <div bind:this={hostEl} class="vault-live-editor__host min-h-0 flex-1"></div>
</div>
