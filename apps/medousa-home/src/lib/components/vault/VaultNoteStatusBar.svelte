<script lang="ts">
  import { vaultNoteStats, formatVaultNoteStats } from "$lib/utils/vaultNoteStats";
  import { sortVaultTagsForDisplay } from "$lib/utils/vaultFrontmatter";
  import { formatShortcut } from "$lib/platform";

  interface Props {
    content: string;
    tags?: string[];
    editorMode?: "edit" | "preview";
    /** Live plane: words · read time only. */
    dense?: boolean;
  }

  let {
    content,
    tags = [],
    editorMode = "preview",
    dense = false,
  }: Props = $props();

  const stats = $derived(vaultNoteStats(content));
  const summary = $derived(formatVaultNoteStats(stats));
  const displayTags = $derived(sortVaultTagsForDisplay(tags).slice(0, 4));
  const extraTagCount = $derived(Math.max(0, tags.length - displayTags.length));
  const tagLine = $derived.by(() => {
    if (displayTags.length === 0) return "";
    const base = displayTags.join(" · ");
    return extraTagCount > 0 ? `${base} · +${extraTagCount}` : base;
  });
</script>

<footer class="vault-note-status" aria-label="Note statistics">
  <p class="vault-note-status-line">
    <span>{summary}</span>
    {#if !dense && tagLine}
      <span class="vault-note-status-sep" aria-hidden="true">·</span>
      <span class="vault-note-status-tags truncate">{tagLine}</span>
    {/if}
    {#if !dense}
      <span class="vault-note-status-sep" aria-hidden="true">·</span>
      <span class="tabular-nums">{stats.characters.toLocaleString()} chars</span>
    {/if}
  </p>
  {#if !dense}
    <p class="vault-note-status-hints">
      {#if editorMode === "preview"}
        <kbd class="vault-kbd">{formatShortcut("F")}</kbd> find
      {:else}
        <kbd class="vault-kbd">{formatShortcut("F")}</kbd> find
        <span class="vault-note-status-sep" aria-hidden="true">·</span>
        <kbd class="vault-kbd">{formatShortcut("S")}</kbd> save
      {/if}
    </p>
  {/if}
</footer>
