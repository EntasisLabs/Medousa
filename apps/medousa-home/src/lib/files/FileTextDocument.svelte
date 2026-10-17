<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import CodeMarkdownPreview from "$lib/components/code/CodeMarkdownPreview.svelte";
  import { projectMarkdownTarget } from "$lib/code/codeMarkdownDocument";
  import { attachmentPreviewUrl } from "$lib/utils/vaultAttachmentPicker";
  import { fileDocuments, type FileDocument } from "./fileDocuments.svelte";
  import { fileLanguage, fileViewKind } from "./fileTypes";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";

  let { path, editable = false }: { path: string; editable?: boolean } = $props();
  let doc = $state<FileDocument | null>(null);
  let error = $state<string | null>(null);
  let editor = $state<CodeMirrorHost>();
  let mode = $state<"source" | "preview" | "split">(untrack(() => fileViewKind(path) === "markdown" ? "preview" : "source"));
  let revision = $state(0);
  const markdown = $derived(fileViewKind(path) === "markdown");
  const urls = new Set<string>();
  let disposed = false;
  const imageRequests = new Map<string, Promise<string>>();
  function absoluteTarget(raw: string) {
    const target = projectMarkdownTarget(path.replace(/\\/g, "/"), raw);
    if (!target) return null;
    return path.startsWith("/") && !target.path.startsWith("/") ? `/${target.path}` : target.path;
  }
  const images = { resolve(raw: string): Promise<string> {
    if (/^(https?:|data:image\/|blob:)/i.test(raw)) return Promise.resolve(raw);
    const target = absoluteTarget(raw);
    if (!target || fileViewKind(target) !== "image") return Promise.reject(new Error("Image unavailable"));
    let request = imageRequests.get(target);
    if (!request) {
      request = attachmentPreviewUrl(target).then((url) => {
        if (!url) throw new Error("Image unavailable");
        if (disposed) { URL.revokeObjectURL(url); throw new Error("Document closed"); }
        urls.add(url); return url;
      });
      imageRequests.set(target, request);
    }
    return request;
  } };

  $effect(() => {
    revision;
    let cancelled = false;
    doc = null; error = null;
    void fileDocuments.open(path).then((value) => {
      if (!cancelled) doc = value;
      else fileDocuments.release(path);
    }).catch((reason) => { if (!cancelled) error = reason instanceof Error ? reason.message : String(reason); });
    return () => { cancelled = true; };
  });
  $effect(() => {
    const unregister = fileDocuments.register(path, () => editor?.flushChanges());
    return () => { editor?.flushChanges(); unregister(); fileDocuments.release(path); };
  });
  onDestroy(() => {
    disposed = true; for (const url of urls) URL.revokeObjectURL(url);
    urls.clear(); imageRequests.clear();
  });
  function choose(next: typeof mode) { editor?.flushChanges(); mode = next; }
  function keydown(event: KeyboardEvent) {
    if (editable && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
      event.preventDefault(); void fileDocuments.save(path);
    }
  }
  function reload() {
    editor?.flushChanges();
    if (fileDocuments.dirty(path) && !window.confirm("Discard this draft and reload the file from disk?")) return;
    fileDocuments.close(path); revision += 1;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex h-full min-h-0 flex-1 flex-col" onkeydown={keydown}>
  <div class="flex shrink-0 items-center justify-between gap-2 border-b border-surface-600/30 px-3 py-2 text-chrome-sm">
    <span class="text-content-tertiary">{editable ? doc && doc.draft !== doc.original ? 'Unsaved changes' : 'Saved on disk' : 'Read-only'}</span>
    <div class="flex items-center gap-2">
      {#if markdown}
        {#each ['preview', 'source', 'split'] as view}
          <button type="button" class="rounded px-2 py-1 hover:bg-surface-700/40" class:bg-surface-700={mode === view} aria-pressed={mode === view} onclick={() => choose(view as typeof mode)}>{view === 'preview' ? 'Preview' : view === 'source' ? 'Source' : 'Split'}</button>
        {/each}
      {/if}
      <button type="button" class="rounded px-2 py-1 hover:bg-surface-700/40" onclick={reload}>Reload</button>
      {#if editable}<button type="button" class="rounded bg-primary-500/20 px-2 py-1 disabled:opacity-50" disabled={!doc || doc.saving || doc.original === doc.draft} onclick={() => void fileDocuments.save(path)}>{doc?.saving ? 'Saving…' : 'Save'}</button>{/if}
    </div>
  </div>
  {#if error || doc?.error}<p class="shrink-0 px-4 py-2 text-sm text-content-warning" role="alert">{error || doc?.error}</p>{/if}
  {#if doc}
    <div class="file-document-panes flex min-h-0 flex-1" class:split={mode === 'split'}>
      {#if mode !== 'preview'}
        <div class="min-h-0 min-w-0 flex-1"><CodeMirrorHost bind:this={editor} value={doc.draft} languageId={fileLanguage(path)} contentSyncKey={revision} readOnly={!editable || doc.saving} onchange={(value) => { if (editable && doc) doc.draft = value; }} /></div>
      {/if}
      {#if markdown && mode !== 'source'}
        <div class="min-h-0 min-w-0 flex-1 overflow-hidden"><CodeMarkdownPreview content={doc.draft} {path} workId="" {images} onOpenLocation={(target) => lmeWorkspace.openFile(path.startsWith('/') && !target.startsWith('/') ? `/${target}` : target)} /></div>
      {/if}
    </div>
  {:else if !error}<p class="p-6 text-sm text-content-tertiary">Loading file…</p>{/if}
</div>
<style>
  .split > div { flex-basis:50%; }
  .split > div + div { border-left:1px solid rgb(var(--theme-border) / .3); }
  @media(max-width:700px) { .file-document-panes.split { flex-direction:column; } .split > div + div { border-left:0; border-top:1px solid rgb(var(--theme-border) / .3); } }
</style>
