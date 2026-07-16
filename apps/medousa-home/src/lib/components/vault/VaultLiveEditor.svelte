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
  import { handleLiveHeadingKey } from "$lib/vault/live/headingKeymap";
  import { handleLiveNavKey } from "$lib/vault/live/liveNavKeymap";
  import type { SlashBlockId } from "$lib/utils/vaultMarkdownEdit";

  interface Props {
    /** Full note markdown (source of truth from parent). */
    value: string;
    /** Document identity — remount parent with {#key} on change; also gates reloads. */
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
  /** Key this editor instance is bound to — never flush if it diverges. */
  let boundKey = "";
  let applyingExternal = false;
  let ready = false;

  const onchangeRef = { current: onchange };
  const onSlashCheckRef = { current: onSlashCheck };
  const onSlashKeyRef = { current: onSlashKey };
  const onEditFenceRef = { current: onEditFenceInBuild };
  const slashOpenRef = { current: slashOpen };
  const boundKeyRef = { current: "" };

  $effect(() => {
    onchangeRef.current = onchange;
    onSlashCheckRef.current = onSlashCheck;
    onSlashKeyRef.current = onSlashKey;
    onEditFenceRef.current = onEditFenceInBuild;
    slashOpenRef.current = slashOpen;
  });

  function emitMarkdown() {
    if (!editor || applyingExternal || !ready) return;
    // Document switched out from under us — do not write stale TipTap into vault.
    if (boundKeyRef.current !== contentSyncKey) return;
    const md = serializeLiveMarkdown(editor.getJSON(), frontmatter);
    onchangeRef.current(md);
  }

  function loadFromMarkdown(md: string) {
    if (!editor) return;
    const parsed = parseLiveMarkdown(md);
    frontmatter = parsed.frontmatter;
    tags = parsed.tags;
    applyingExternal = true;
    ready = false;
    editor.commands.setContent(parsed.doc, { contentType: "json" });
    applyingExternal = false;
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
    boundKey = contentSyncKey;
    boundKeyRef.current = contentSyncKey;

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

          if (editor && handleLiveNavKey(editor, event)) {
            return true;
          }

          if (editor && handleLiveHeadingKey(editor, event)) {
            return true;
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

    queueMicrotask(() => {
      ready = true;
      // Mount-before-hydrate: parent may fill value after TipTap created.
      if (value !== initial) {
        loadFromMarkdown(value);
      }
    });
  });

  $effect(() => {
    if (!editor) return;
    editor.setEditable(!disabled);
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
    // Same document: catch mount-before-hydrate only.
    if (next.trim() && editor.isEmpty) {
      loadFromMarkdown(next);
    }
  });

  onDestroy(() => {
    // Never flush on destroy — note switches remount with a new key; flushing
    // here would write the previous note's TipTap doc onto the new vault.content.
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
