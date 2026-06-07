<script lang="ts">
  import type { Surface } from "$lib/types/ui";

  interface Props {
    active: Surface;
    onSelect: (surface: Surface) => void;
  }

  let { active, onSelect }: Props = $props();

  const items: { id: Surface; label: string; icon: string; disabled?: boolean }[] =
    [
      { id: "home", label: "Home", icon: "⌂" },
      { id: "chat", label: "Chat", icon: "◎" },
      { id: "library", label: "Library", icon: "▤" },
      { id: "work", label: "Work", icon: "▦" },
      { id: "settings", label: "Settings", icon: "⚙", disabled: true },
    ];
</script>

<nav
  class="flex h-full w-12 shrink-0 flex-col items-center gap-2 border-r border-surface-500/20 bg-surface-900 py-3"
  aria-label="Primary navigation"
>
  {#each items as item}
    <button
      type="button"
      class="btn btn-sm w-9 h-9 p-0 rounded-container-token {active === item.id
        ? 'variant-filled-primary'
        : 'variant-ghost-surface'}"
      title={item.label}
      disabled={item.disabled}
      onclick={() => onSelect(item.id)}
    >
      <span class="text-base leading-none">{item.icon}</span>
    </button>
  {/each}
</nav>
