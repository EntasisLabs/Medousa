<script lang="ts">
  import {
    kindBadgeClass,
    kindLabel,
    normalizeKind,
    resolveKind,
    type VaultNoteKind,
  } from "$lib/utils/vaultFrontmatter";

  interface Props {
    kind?: string | null;
    path?: string | null;
    /** Compact chip for tree rows. */
    compact?: boolean;
  }

  let { kind = null, path = null, compact = false }: Props = $props();

  const resolved = $derived(
    path ? resolveKind(path, kind) : normalizeKind(kind),
  );

  const label = $derived(kindLabel(resolved));
  const badgeClass = $derived(kindBadgeClass(resolved));
</script>

<span
  class="badge {badgeClass} shrink-0 font-medium {compact
    ? 'px-1.5 py-0 text-[10px] leading-4'
    : 'text-xs'}"
  title="Note kind: {label}"
>
  {label}
</span>
