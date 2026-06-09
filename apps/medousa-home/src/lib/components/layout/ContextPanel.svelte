<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import type { WorkCardDetail } from "$lib/types/card";
  import { formatCardTitle } from "$lib/utils/formatWork";
  import { vaultDisplayTitle, wikilinkLabel } from "$lib/utils/formatVault";

  interface Props {
    notePath: string | null;
    noteTitle: string | null;
    wikilinksOut: string[];
    backlinks: string[];
    cardDetail: WorkCardDetail | null;
    cardError: string | null;
    noteDiffChip: string | null;
    onOpenNote: (path: string) => void;
  }

  let {
    notePath,
    noteTitle,
    wikilinksOut,
    backlinks,
    cardDetail,
    cardError,
    noteDiffChip,
    onOpenNote,
  }: Props = $props();

  const cardVaultPaths = $derived(cardDetail?.associations.vault_paths ?? []);
  const hasNoteContext = $derived(
    notePath !== null &&
      (wikilinksOut.length > 0 || backlinks.length > 0 || noteDiffChip !== null),
  );
  const titleByPath = $derived(vault.labelByPath());

  const hasCardContext = $derived(
    cardDetail !== null &&
      (cardVaultPaths.length > 0 ||
        Boolean(cardDetail.result_excerpt) ||
        Boolean(cardDetail.subtitle)),
  );
</script>

{#if hasCardContext || hasNoteContext}
  <section
    class="min-w-0 shrink-0 border-b border-surface-500/45 bg-surface-800/40 px-4 py-3"
    aria-label="Context"
  >
    <h3 class="workshop-section-title">Context</h3>

    {#if cardDetail}
      <div class="mt-2 space-y-2 text-sm">
        <p class="font-medium text-surface-100">
          {formatCardTitle(cardDetail.card)}
        </p>
        {#if cardDetail.subtitle}
          <p class="workshop-faint">{cardDetail.subtitle}</p>
        {/if}
        {#if cardDetail.result_excerpt}
          <p class="workshop-inset p-2 text-xs leading-relaxed text-surface-100">
            {cardDetail.result_excerpt}
          </p>
        {/if}
        {#if cardVaultPaths.length > 0}
          <div>
            <p class="workshop-label mb-1">Linked notes</p>
            <ul class="space-y-1">
              {#each cardVaultPaths as path (path)}
                <li>
                  <button
                    type="button"
                    class="text-left text-xs text-primary-400 hover:underline"
                    onclick={() => onOpenNote(path)}
                  >
                    {vaultDisplayTitle(titleByPath.get(path) ?? path, path)}
                  </button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    {/if}

    {#if cardError}
      <p class="mt-2 text-xs text-error-400">{cardError}</p>
    {/if}

    {#if notePath}
      <div class="mt-3 space-y-2 text-sm">
        <div class="flex items-center gap-2">
          <p class="workshop-faint">
            {vaultDisplayTitle(noteTitle ?? notePath, notePath)}
          </p>
          {#if noteDiffChip}
            <span class="badge variant-soft-warning text-[10px] font-mono">
              {noteDiffChip}
            </span>
          {/if}
        </div>
        {#if wikilinksOut.length > 0}
          <div>
            <p class="workshop-label mb-1">Links out</p>
            <ul class="space-y-1">
              {#each wikilinksOut as link (link)}
                <li>
                  <button
                    type="button"
                    class="text-left text-xs text-primary-400 hover:underline"
                    onclick={() => onOpenNote(link)}
                  >
                    {wikilinkLabel(link, titleByPath)}
                  </button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        {#if backlinks.length > 0}
          <div>
            <p class="workshop-label mb-1">Backlinks</p>
            <ul class="space-y-1">
              {#each backlinks as link (link)}
                <li>
                  <button
                    type="button"
                    class="text-left text-xs text-primary-400 hover:underline"
                    onclick={() => onOpenNote(link)}
                  >
                    {wikilinkLabel(link, titleByPath)}
                  </button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    {/if}
  </section>
{/if}
