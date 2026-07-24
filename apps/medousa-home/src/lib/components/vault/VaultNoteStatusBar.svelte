<script lang="ts">
  import { vaultNoteStats, formatVaultNoteStats } from "$lib/utils/vaultNoteStats";
  import { sortVaultTagsForDisplay } from "$lib/utils/vaultFrontmatter";
  import { formatShortcut } from "$lib/platform";
  import { vaultVersions } from "$lib/stores/vaultVersions.svelte";

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
  const versionsLabel = $derived.by(() => {
    if (!vaultVersions.enabled || !vaultVersions.status?.isRepo) return "";
    const branch = vaultVersions.status.branch ?? "main";
    const dirty =
      vaultVersions.status.dirtyCount > 0
        ? ` · ${vaultVersions.status.dirtyCount} changed`
        : "";
    return `${branch}${dirty}`;
  });

  $effect(() => {
    if (!vaultVersions.enabled) return;
    void vaultVersions.refresh();
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
    {#if versionsLabel}
      <span class="vault-note-status-sep" aria-hidden="true">·</span>
      <button
        type="button"
        class="vault-note-status-versions truncate text-left hover:text-surface-100"
        title="Open Versions"
        onclick={() => vaultVersions.openPanel()}
      >
        {versionsLabel}
      </button>
    {/if}
  </p>
  {#if !dense}
    <p class="vault-note-status-hints">
      {#if editorMode === "preview"}
        <kbd class="vault-kbd">{formatShortcut("F")}</kbd> find
        <span class="vault-note-status-sep" aria-hidden="true">·</span>
        <kbd class="vault-kbd">{formatShortcut("N")}</kbd> new
      {:else}
        <kbd class="vault-kbd">{formatShortcut("F")}</kbd> find
        <span class="vault-note-status-sep" aria-hidden="true">·</span>
        <kbd class="vault-kbd">{formatShortcut("S")}</kbd> save
        <span class="vault-note-status-sep" aria-hidden="true">·</span>
        <kbd class="vault-kbd">{formatShortcut("N")}</kbd> new
      {/if}
    </p>
  {/if}
</footer>
