<script lang="ts">
  import "$lib/styles/vault-live.postcss";
  import "$lib/styles/vault-editor.postcss";
  import { onMount } from "svelte";
  import { Editor } from "@tiptap/core";
  import Image from "@tiptap/extension-image";
  import { createLiveExtensions } from "$lib/vault/live/liveExtensions";
  import { applyLiveFormatAction, applyLiveTextColor, liveActiveFormatActions } from "$lib/vault/live/liveSelectionFormat";
  import type { MarkdownFormatAction } from "$lib/utils/vaultMarkdownEdit";
  import { MarkdownSourceDocument } from "$lib/markdown/document/markdownSourceDocument";
  import { MarkdownSourceBlock, MarkdownSourceIdentity } from "$lib/markdown/document/markdownSourceExtensions";
  import { projectMarkdownTarget, type ProjectMarkdownImages } from "$lib/code/codeMarkdownDocument";
  import VaultFormatBar from "$lib/components/vault/VaultFormatBar.svelte";
  import { handleLiveNavKey } from "$lib/vault/live/liveNavKeymap";
  import { handleLiveHeadingKey } from "$lib/vault/live/headingKeymap";

  let { value, path, workId, disabled, images, onchange, onOpenLocation }: { value: string; path: string; workId: string; disabled: boolean; images: ProjectMarkdownImages; onchange: (value: string) => void; onOpenLocation: (path: string, line: number) => void } = $props();
  let host: HTMLDivElement;
  let editor = $state<Editor>();
  let activeActions = $state<MarkdownFormatAction[]>([]);
  let source: MarkdownSourceDocument;
  let current = "";
  let applying = false;

  onMount(() => {
    source = new MarkdownSourceDocument(value);
    current = value;
    const projectImage = Image.extend({
      addNodeView() {
        return ({ node }) => {
          const dom = document.createElement("figure");
          const image = document.createElement("img");
          image.alt = String(node.attrs.alt ?? "");
          dom.append(image);
          let alive = true;
          let generation = 0;
          const load = (raw: string) => {
            const current = ++generation;
            image.removeAttribute("src");
            void images.resolve(raw).then((url) => { if (alive && current === generation) image.src = url; }).catch(() => { if (alive && current === generation) { image.alt = `Image unavailable: ${node.attrs.alt || raw}`; image.title = image.alt; } });
          };
          load(String(node.attrs.src));
          return { dom, destroy() { alive = false; }, update(next) {
            if (next.type !== node.type) return false;
            if (next.attrs.src !== node.attrs.src) load(String(next.attrs.src));
            image.alt = String(next.attrs.alt ?? "");
            node = next;
            return true;
          } };
        };
      },
    }).configure({ allowBase64: true });
    editor = new Editor({
      element: host,
      extensions: [...createLiveExtensions({ hideMarkdownSyntax: () => true, trailingNode: false, headingLevels: [1, 2, 3, 4, 5, 6], placeholder: "Start writing…" }).filter((extension) => extension.name !== "image"), projectImage, MarkdownSourceIdentity, MarkdownSourceBlock],
      content: source.doc,
      editable: !disabled,
      editorProps: {
        attributes: { class: "vault-live-prose", "aria-label": "Markdown editor", role: "textbox", "aria-multiline": "true" },
        handleKeyDown: (_view, event) => editor ? handleLiveNavKey(editor, event) || handleLiveHeadingKey(editor, event) : false,
        handleClick: (_view, _pos, event) => {
          if (!(event.metaKey || event.ctrlKey) || !(event.target instanceof Element)) return false;
          const href = event.target.closest("a[href]")?.getAttribute("href");
          if (!href) return false;
          event.preventDefault();
          const target = projectMarkdownTarget(path, href);
          if (target) onOpenLocation(target.path, 1);
          else if (/^https?:/i.test(href)) void import("$lib/utils/openInBrowser").then(({ openInBrowser }) => openInBrowser(href, { workCardId: workId }));
          return true;
        },
      },
      onTransaction: ({ editor }) => { activeActions = liveActiveFormatActions(editor); },
      onUpdate: ({ editor }) => {
        if (applying) return;
        // Organism node views can dispatch their own transactions. Custody also
        // applies to those callbacks, independently of contenteditable.
        if (disabled) {
          applying = true;
          source = new MarkdownSourceDocument(current);
          editor.commands.setContent(source.doc, { emitUpdate: false });
          source.establishBaseline(editor.getJSON());
          applying = false;
          return;
        }
        const next = source.serialize(editor.getJSON());
        if (next !== current) { current = next; onchange(next); }
      },
    });
    // Establish synchronously: a save can happen before TipTap's onCreate callback.
    source.establishBaseline(editor.getJSON());
    return () => { editor?.destroy(); };
  });

  $effect(() => { editor?.setEditable(!disabled, false); });
  $effect(() => {
    const next = value;
    if (!editor || next === current) return;
    current = next;
    applying = true;
    source = new MarkdownSourceDocument(next);
    editor.commands.setContent(source.doc, { emitUpdate: false });
    source.establishBaseline(editor.getJSON());
    applying = false;
  });

  export function focusEditor() { editor?.commands.focus(undefined, { scrollIntoView: false }); }
</script>

<div class="code-markdown-writer flex h-full min-h-0 flex-col" class:read-only={disabled}>
  <VaultFormatBar compact={false} {disabled} {activeActions} onFormat={(action) => { if (editor) applyLiveFormatAction(editor, action, undefined, true); }} onColor={(color) => { if (editor) applyLiveTextColor(editor, color); }} />
  <div class="vault-live-editor min-h-0 flex-1 overflow-auto">
    <div class="writer-document mx-auto max-w-3xl" bind:this={host}></div>
  </div>
</div>

<style>
  .writer-document { max-width: 48rem; margin-inline: auto; }
  :global(.code-markdown-writer.read-only .vault-live-organism-host) { pointer-events: none; }
  :global(.code-markdown-writer .ProseMirror) { min-height: 100%; outline: none; }
  :global(.code-markdown-writer .markdown-source-block) { border: 1px solid rgb(var(--color-surface-500) / .35); border-radius: .5rem; padding: .75rem; margin-block: 1rem; }
  :global(.code-markdown-writer .markdown-source-block button) { color: rgb(var(--color-primary-300)); font-size: .75rem; margin-bottom: .5rem; }
  :global(.code-markdown-writer .markdown-source-block pre) { white-space: pre-wrap; font: .8rem/1.6 var(--font-family-mono, monospace); }
  :global(.code-markdown-writer .markdown-source-block textarea) { width: 100%; min-height: 8rem; resize: vertical; background: transparent; color: inherit; font: .8rem/1.6 var(--font-family-mono, monospace); }
</style>
