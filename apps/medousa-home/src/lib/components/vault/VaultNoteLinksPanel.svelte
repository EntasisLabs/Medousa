<script lang="ts">
  import { wikilinkLabel } from "$lib/utils/formatVault";

  interface Props {
    wikilinksOut: string[];
    backlinks: string[];
    labelByPath: Map<string, string>;
    onOpenNote: (path: string) => void;
  }

  let { wikilinksOut, backlinks, labelByPath, onOpenNote }: Props = $props();

  const hasLinks = $derived(wikilinksOut.length > 0 || backlinks.length > 0);
</script>

{#if hasLinks}
  <aside
    class="vault-links-panel flex w-[220px] shrink-0 flex-col border-l border-surface-500/40 bg-surface-900/40"
    aria-label="Note links"
  >
    <div class="border-b border-surface-500/35 px-3 py-2">
      <h3 class="text-[11px] font-semibold uppercase tracking-wide text-surface-400">
        Links
      </h3>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto px-3 py-2 text-sm">
      {#if wikilinksOut.length > 0}
        <section class="mb-4">
          <p class="mb-1.5 text-[10px] font-medium uppercase tracking-wide text-surface-500">
            Out
          </p>
          <ul class="space-y-1">
            {#each wikilinksOut as link (link)}
              <li>
                <button
                  type="button"
                  class="vault-link-chip w-full text-left"
                  onclick={() => onOpenNote(link)}
                >
                  {wikilinkLabel(link, labelByPath)}
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if backlinks.length > 0}
        <section>
          <p class="mb-1.5 text-[10px] font-medium uppercase tracking-wide text-surface-500">
            Back
          </p>
          <ul class="space-y-1">
            {#each backlinks as link (link)}
              <li>
                <button
                  type="button"
                  class="vault-link-chip w-full text-left"
                  onclick={() => onOpenNote(link)}
                >
                  {labelByPath.get(link) ?? link}
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    </div>
  </aside>
{/if}
