<script lang="ts">
  import { vaultNoteStats, formatVaultNoteStats } from "$lib/utils/vaultNoteStats";
  import { sortVaultTagsForDisplay } from "$lib/utils/vaultFrontmatter";

  interface Props {
    content: string;
    tags?: string[];
    editorMode?: "edit" | "preview";
  }

  let { content, tags = [], editorMode = "preview" }: Props = $props();

  const stats = $derived(vaultNoteStats(content));
  const summary = $derived(formatVaultNoteStats(stats));
  const displayTags = $derived(sortVaultTagsForDisplay(tags).slice(0, 6));
  const extraTagCount = $derived(Math.max(0, tags.length - displayTags.length));
</script>

<footer class="vault-note-status workshop-status" aria-label="Note statistics">
  <span class="workshop-status-whisper text-surface-400">
    <span class="workshop-status-dot workshop-status-dot-muted" aria-hidden="true"></span>
    <span class="text-surface-300">{summary}</span>
  </span>

  <div class="flex min-w-0 flex-1 items-center gap-2 overflow-hidden px-3">
    {#if displayTags.length > 0}
      <ul class="flex min-w-0 flex-wrap items-center gap-1" aria-label="Semantic tags">
        {#each displayTags as tag (tag)}
          <li>
            <span class="badge preset-tonal-surface text-[10px] font-medium">{tag}</span>
          </li>
        {/each}
        {#if extraTagCount > 0}
          <li class="text-[10px] text-surface-500">+{extraTagCount}</li>
        {/if}
      </ul>
    {/if}
  </div>

  <div class="flex shrink-0 items-center gap-3 text-surface-500">
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
