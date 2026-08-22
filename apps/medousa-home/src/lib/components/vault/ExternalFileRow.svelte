<script lang="ts">
  import { File, FileArchive, FileCode2, FileImage, FileText, Link2 } from "@lucide/svelte";
  import type { ExternalFileEntry } from "$lib/types/externalDesk";
  import {
    formatExternalFileSize,
    formatExternalModified,
  } from "$lib/utils/externalDeskApi";

  interface Props {
    entry: ExternalFileEntry;
    selected?: boolean;
    showLink?: boolean;
    disabled?: boolean;
    onOpen: (entry: ExternalFileEntry) => void;
    onLink?: (entry: ExternalFileEntry) => void;
  }

  let {
    entry,
    selected = false,
    showLink = false,
    disabled = false,
    onOpen,
    onLink,
  }: Props = $props();

  const kind = $derived.by(() => {
    const ext = (entry.ext ?? entry.name.split(".").pop() ?? "")
      .toLowerCase()
      .replace(/^\./, "");
    if (["png", "jpg", "jpeg", "gif", "webp", "svg", "heic", "avif"].includes(ext)) {
      return "image";
    }
    if (["zip", "tar", "gz", "bz2", "7z", "rar"].includes(ext)) return "archive";
    if (
      ["js", "ts", "tsx", "jsx", "rs", "py", "go", "java", "c", "cpp", "h", "css", "html", "json", "toml", "yaml", "yml"].includes(ext)
    ) {
      return "code";
    }
    if (["pdf", "md", "txt", "rtf", "doc", "docx"].includes(ext)) return "document";
    return "file";
  });
</script>

<div
  class="vault-external-file-shell group {selected ? 'vault-external-file-shell-selected' : ''}"
>
  <button
    type="button"
    class="vault-external-file-row min-w-0 flex-1 text-left"
    {disabled}
    title={entry.path}
    onclick={() => onOpen(entry)}
  >
    <span class="vault-external-file-icon" aria-hidden="true">
      {#if kind === "image"}
        <FileImage size={14} strokeWidth={1.7} />
      {:else if kind === "archive"}
        <FileArchive size={14} strokeWidth={1.7} />
      {:else if kind === "code"}
        <FileCode2 size={14} strokeWidth={1.7} />
      {:else if kind === "document"}
        <FileText size={14} strokeWidth={1.7} />
      {:else}
        <File size={14} strokeWidth={1.7} />
      {/if}
    </span>
    <span class="vault-external-file-copy">
      <span class="vault-external-file-name">{entry.name}</span>
      <span class="vault-external-file-meta">
        {formatExternalModified(entry.modified_at_utc)}
        {#if entry.size_bytes > 0}
          · {formatExternalFileSize(entry.size_bytes)}
        {/if}
      </span>
    </span>
  </button>
  {#if showLink && onLink}
    <button
      type="button"
      class="vault-external-file-link opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100"
      aria-label="Link to open note"
      title="Link to note"
      {disabled}
      onclick={(event) => {
        event.stopPropagation();
        onLink(entry);
      }}
    >
      <Link2 size={13} strokeWidth={2} />
    </button>
  {/if}
</div>
