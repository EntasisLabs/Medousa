<script lang="ts">
  import {
    CHEAT_SHEET_GROUP_IDS,
    KEYBOARD_SHORTCUTS_CATALOG,
    formatCatalogKeys,
  } from "$lib/utils/keyboardShortcutsCatalog";

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  const groups = $derived(
    CHEAT_SHEET_GROUP_IDS.map((id) =>
      KEYBOARD_SHORTCUTS_CATALOG.find((group) => group.id === id),
    ).filter((group): group is NonNullable<typeof group> => Boolean(group)),
  );
</script>

<button
  type="button"
  class="shell-pane-cheatsheet-backdrop absolute inset-0 z-40 bg-black/40"
  aria-label="Close keyboard shortcuts"
  onclick={onClose}
></button>
<div
  class="shell-pane-cheatsheet absolute left-1/2 top-1/2 z-50 max-h-[min(32rem,85vh)] w-[min(24rem,90vw)] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-lg border border-surface-500/40 bg-surface-900 p-4 shadow-xl"
  role="dialog"
  aria-modal="true"
  aria-label="Keyboard shortcuts"
>
  <div class="mb-3 flex items-center justify-between gap-2">
    <h2 class="text-sm font-semibold text-surface-50">Keyboard shortcuts</h2>
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={onClose}>
      Close
    </button>
  </div>
  <div class="space-y-4">
    {#each groups as group (group.id)}
      <section>
        <h3 class="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-surface-500">
          {group.title}
        </h3>
        <ul class="space-y-1.5">
          {#each group.entries as row (row.id)}
            <li class="flex items-baseline justify-between gap-3 text-xs">
              <kbd class="shrink-0 rounded bg-surface-800 px-1.5 py-0.5 font-mono text-surface-200">
                {formatCatalogKeys(row.keys)}
              </kbd>
              <span class="text-right text-surface-400">{row.action}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  </div>
  <p class="mt-3 text-[11px] text-surface-500">
    Up to 4 live chats. Also in Spotlight — search “keyboard shortcuts”.
  </p>
</div>
