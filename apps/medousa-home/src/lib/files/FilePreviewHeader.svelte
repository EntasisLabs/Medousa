<script lang="ts">
  import "$lib/styles/files-surface.postcss";
  import { ChevronRight, ExternalLink, X } from "@lucide/svelte";
  import CodeFileIcon from "$lib/components/lme/explorers/CodeFileIcon.svelte";
  import { fileExtension, fileViewKind } from "./fileTypes";

  let { path, title, onOpenExternal, onClose }: {
    path: string;
    title: string;
    onOpenExternal: () => void | Promise<void>;
    onClose?: () => void;
  } = $props();
  const folders = $derived(path.replace(/\\/g, "/").split("/").filter(Boolean).slice(0, -1));
  const location = $derived(folders.slice(-3));
  const format = $derived(fileViewKind(path) === "markdown" ? "Markdown" : fileExtension(path).toUpperCase() || "File");
</script>

<header class="files-preview-header">
  <span class="files-preview-file-icon"><CodeFileIcon {path} size={23} /></span>
  <div class="files-preview-heading">
    <div class="files-preview-title-line"><h2 title={title}>{title}</h2><span class="files-format-badge">{format}</span></div>
    <div class="files-location" title={path} aria-label="File location">
      {#if folders.length > 3}<span>…</span><ChevronRight size={10} />{/if}
      {#each location as folder, index}
        {#if index > 0}<ChevronRight size={10} />{/if}<span>{folder}</span>
      {/each}
    </div>
  </div>
  <button type="button" class="files-button files-button--quiet files-open-external" onclick={() => void onOpenExternal()} title="Open in the default app"><ExternalLink size={13} /><span>Open in app</span></button>
  {#if onClose}<button type="button" class="files-icon-button" aria-label="Close preview" onclick={onClose}><X size={15} /></button>{/if}
</header>
