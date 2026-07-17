<script lang="ts">
  import {
    kindBadgeClass,
    kindLabel,
    normalizeKind,
    resolveKind,
    VAULT_KIND_OPTIONS,
    type VaultNoteKind,
  } from "$lib/utils/vaultFrontmatter";

  interface Props {
    kind?: string | null;
    path?: string | null;
    /** Compact chip for tree rows. */
    compact?: boolean;
    /** Chrome pill: open a quiet kind menu. */
    interactive?: boolean;
    disabled?: boolean;
    onKindChange?: (kind: VaultNoteKind) => void;
  }

  let {
    kind = null,
    path = null,
    compact = false,
    interactive = false,
    disabled = false,
    onKindChange,
  }: Props = $props();

  let open = $state(false);

  const resolved = $derived(
    path ? resolveKind(path, kind) : normalizeKind(kind),
  );

  const label = $derived(kindLabel(resolved));
  const badgeClass = $derived(kindBadgeClass(resolved));
  const canSelect = $derived(Boolean(interactive && onKindChange && !disabled));

  function selectKind(next: VaultNoteKind) {
    open = false;
    if (next === resolved) return;
    onKindChange?.(next);
  }
</script>

{#if canSelect}
  <div class="vault-kind-badge relative shrink-0">
    <button
      type="button"
      class="badge {badgeClass} vault-kind-badge__trigger shrink-0 font-medium {compact
        ? 'px-1.5 py-0 text-[10px] leading-4'
        : 'text-xs'}"
      title="Note kind: {label}"
      aria-label="Note kind: {label}. Change kind"
      aria-expanded={open}
      aria-haspopup="menu"
      onclick={() => {
        open = !open;
      }}
    >
      {label}
    </button>

    {#if open}
      <button
        type="button"
        class="fixed inset-0 z-40 cursor-default"
        aria-label="Close kind menu"
        onclick={() => {
          open = false;
        }}
      ></button>
      <div
        class="vault-kind-badge__menu absolute right-0 top-full z-50 mt-1 min-w-[8.5rem] rounded-container-token border border-surface-500/40 bg-surface-900 py-1 shadow-lg"
        role="menu"
      >
        {#each VAULT_KIND_OPTIONS as option (option)}
          <button
            type="button"
            class="vault-kind-badge__item"
            class:vault-kind-badge__item--active={option === resolved}
            role="menuitemradio"
            aria-checked={option === resolved}
            disabled={option === resolved}
            onclick={() => selectKind(option)}
          >
            {kindLabel(option)}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{:else}
  <span
    class="badge {badgeClass} shrink-0 font-medium {compact
      ? 'px-1.5 py-0 text-[10px] leading-4'
      : 'text-xs'}"
    title="Note kind: {label}"
  >
    {label}
  </span>
{/if}
