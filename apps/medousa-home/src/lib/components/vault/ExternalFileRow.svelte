<script lang="ts">
  import { Link2 } from "@lucide/svelte";
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
</script>

<div
  class="group flex items-center gap-1 rounded-container-token {selected
    ? 'bg-primary-500/10'
    : 'hover:bg-surface-700/70'}"
>
  <button
    type="button"
    class="vault-external-file-row min-w-0 flex-1 text-left"
    {disabled}
    title={entry.path}
    onclick={() => onOpen(entry)}
  >
    <span class="block truncate text-sm text-surface-100">{entry.name}</span>
    <span class="workshop-faint block truncate text-[10px]">
      {formatExternalModified(entry.modified_at_utc)}
      {#if entry.size_bytes > 0}
        · {formatExternalFileSize(entry.size_bytes)}
      {/if}
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
