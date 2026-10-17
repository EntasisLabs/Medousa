<script lang="ts">
  import { Link2 } from "@lucide/svelte";
  import CodeFileIcon from "$lib/components/lme/explorers/CodeFileIcon.svelte";
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
    compact?: boolean;
    onOpen: (entry: ExternalFileEntry) => void;
    onLink?: (entry: ExternalFileEntry) => void;
  }

  let {
    entry,
    selected = false,
    showLink = false,
    disabled = false,
    compact = false,
    onOpen,
    onLink,
  }: Props = $props();

</script>

<div
  class="vault-external-file-shell group {selected ? 'vault-external-file-shell-selected' : ''}"
  class:files-row--compact={compact}
>
  <button
    type="button"
    class="vault-external-file-row min-w-0 flex-1 text-left"
    {disabled}
    aria-current={selected ? "true" : undefined}
    title={entry.path}
    onclick={() => onOpen(entry)}
  >
    <span class="vault-external-file-icon" aria-hidden="true">
      <CodeFileIcon path={entry.path} size={compact ? 15 : 17} />
    </span>
    <span class="vault-external-file-copy">
      <span class="vault-external-file-name">{entry.name}</span>
      {#if !compact}<span class="vault-external-file-meta">
        {entry.modified_at_utc ? formatExternalModified(entry.modified_at_utc) : ''}
        {#if entry.size_bytes > 0}
          · {formatExternalFileSize(entry.size_bytes)}
        {/if}
      </span>{/if}
    </span>
    {#if compact}<span class="files-row-size">{formatExternalFileSize(entry.size_bytes)}</span>{/if}
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
