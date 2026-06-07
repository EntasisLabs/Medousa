<script lang="ts">
  import type { WorkCardDetail } from "$lib/types/card";

  interface Props {
    notePath: string | null;
    noteTitle: string | null;
    wikilinksOut: string[];
    backlinks: string[];
    cardDetail: WorkCardDetail | null;
    cardError: string | null;
    onOpenNote: (path: string) => void;
  }

  let {
    notePath,
    noteTitle,
    wikilinksOut,
    backlinks,
    cardDetail,
    cardError,
    onOpenNote,
  }: Props = $props();

  const cardVaultPaths = $derived(cardDetail?.associations.vault_paths ?? []);
  const hasNoteContext =
    notePath !== null && (wikilinksOut.length > 0 || backlinks.length > 0);
  const hasCardContext =
    cardDetail !== null &&
    (cardVaultPaths.length > 0 ||
      Boolean(cardDetail.result_excerpt) ||
      Boolean(cardDetail.subtitle));
</script>

{#if hasCardContext || hasNoteContext}
  <section class="border-b border-surface-500/20 px-4 py-3" aria-label="Context">
    <h3 class="text-xs font-semibold uppercase tracking-wide text-surface-400">
      Context
    </h3>

    {#if cardDetail}
      <div class="mt-2 space-y-2 text-sm">
        <p class="font-medium text-surface-100">{cardDetail.card.title}</p>
        {#if cardDetail.subtitle}
          <p class="text-xs text-surface-400">{cardDetail.subtitle}</p>
        {/if}
        {#if cardDetail.result_excerpt}
          <p class="rounded-container-token bg-surface-800/60 p-2 text-xs leading-relaxed text-surface-200">
            {cardDetail.result_excerpt}
          </p>
        {/if}
        {#if cardVaultPaths.length > 0}
          <div>
            <p class="mb-1 text-xs text-surface-400">Linked notes</p>
            <ul class="space-y-1">
              {#each cardVaultPaths as path (path)}
                <li>
                  <button
                    type="button"
                    class="text-left text-xs text-primary-400 hover:underline"
                    onclick={() => onOpenNote(path)}
                  >
                    {path}
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
        <p class="text-xs text-surface-400">{noteTitle ?? notePath}</p>
        {#if wikilinksOut.length > 0}
          <div>
            <p class="mb-1 text-xs text-surface-400">Links out</p>
            <ul class="space-y-1">
              {#each wikilinksOut as link (link)}
                <li>
                  <button
                    type="button"
                    class="text-left text-xs text-primary-400 hover:underline"
                    onclick={() => onOpenNote(link)}
                  >
                    [[{link}]]
                  </button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        {#if backlinks.length > 0}
          <div>
            <p class="mb-1 text-xs text-surface-400">Backlinks</p>
            <ul class="space-y-1">
              {#each backlinks as link (link)}
                <li>
                  <button
                    type="button"
                    class="text-left text-xs text-primary-400 hover:underline"
                    onclick={() => onOpenNote(link)}
                  >
                    {link}
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
