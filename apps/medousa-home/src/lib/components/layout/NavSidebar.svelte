<script lang="ts">
  import {
    Activity,
    BookOpen,
    Home,
    LayoutGrid,
    MessageCircle,
    Settings,
    Zap,
  } from "@lucide/svelte";
  import type { Component } from "svelte";
  import type { Surface } from "$lib/types/ui";

  interface Props {
    active: Surface;
    onSelect: (surface: Surface) => void;
  }

  let { active, onSelect }: Props = $props();

  const items: { id: Surface; label: string; icon: Component }[] = [
    { id: "home", label: "Home", icon: Home },
    { id: "chat", label: "Chat", icon: MessageCircle },
    { id: "library", label: "Library", icon: BookOpen },
    { id: "skills", label: "Skills", icon: Zap },
    { id: "work", label: "Work", icon: LayoutGrid },
    { id: "runtime", label: "Runtime", icon: Activity },
  ];

  const iconProps = { size: 20, strokeWidth: 1.75 };
</script>

<nav
  class="flex h-full w-44 shrink-0 flex-col border-r border-surface-500/20 bg-surface-900/90"
  aria-label="Primary navigation"
>
  <div class="border-b border-surface-500/20 px-4 py-4">
    <p class="text-sm font-semibold tracking-wide text-surface-100">Medousa</p>
    <p class="mt-0.5 text-[11px] text-surface-500">The Workshop</p>
  </div>

  <div class="flex flex-1 flex-col gap-1 p-2">
    {#each items as item (item.id)}
      {@const Icon = item.icon}
      <button
        type="button"
        class="flex w-full items-center gap-3 rounded-container-token px-3 py-2.5 text-left text-sm transition {active ===
        item.id
          ? 'bg-primary-500/15 font-medium text-primary-200'
          : 'text-surface-300 hover:bg-surface-800/80 hover:text-surface-100'}"
        onclick={() => onSelect(item.id)}
      >
        <span
          class="flex h-7 w-7 shrink-0 items-center justify-center rounded-token {active ===
          item.id
            ? 'bg-primary-500/25 text-primary-100'
            : 'bg-surface-800 text-surface-400'}"
        >
          <Icon {...iconProps} />
        </span>
        {item.label}
      </button>
    {/each}
  </div>

  <div class="border-t border-surface-500/20 p-2">
    <button
      type="button"
      class="flex w-full items-center gap-3 rounded-container-token px-3 py-2.5 text-left text-sm transition {active ===
      'settings'
        ? 'bg-primary-500/15 font-medium text-primary-200'
        : 'text-surface-300 hover:bg-surface-800/80 hover:text-surface-100'}"
      onclick={() => onSelect("settings")}
    >
      <span
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded-token {active ===
        'settings'
          ? 'bg-primary-500/25 text-primary-100'
          : 'bg-surface-800 text-surface-400'}"
      >
        <Settings {...iconProps} />
      </span>
      Settings
    </button>
  </div>
</nav>
