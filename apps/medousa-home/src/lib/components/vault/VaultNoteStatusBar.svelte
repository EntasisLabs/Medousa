<script lang="ts">
  import { vaultNoteStats, formatVaultNoteStats } from "$lib/utils/vaultNoteStats";

  interface Props {
    content: string;
    editorMode?: "edit" | "preview";
  }

  let { content, editorMode = "preview" }: Props = $props();

  const stats = $derived(vaultNoteStats(content));
  const summary = $derived(formatVaultNoteStats(stats));
</script>

<footer class="vault-note-status workshop-status" aria-label="Note statistics">
  <span class="workshop-status-whisper text-surface-400">
    <span class="workshop-status-dot workshop-status-dot-muted" aria-hidden="true"></span>
    <span class="text-surface-300">{summary}</span>
  </span>

  <div class="flex items-center gap-3 text-surface-500">
    <span>{stats.characters.toLocaleString()} characters</span>
    {#if editorMode === "preview"}
      <span class="hidden sm:inline">
        <kbd class="vault-kbd">⌘F</kbd> find
      </span>
    {:else}
      <span class="hidden sm:inline">
        <kbd class="vault-kbd">⌘F</kbd> find · <kbd class="vault-kbd">⌘S</kbd> save
      </span>
    {/if}
  </div>
</footer>
