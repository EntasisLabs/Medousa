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

  const iconProps = { size: 18, strokeWidth: 1.75 };
</script>

<nav class="workshop-icon-rail" aria-label="Primary navigation">
  <button
    type="button"
    class="workshop-rail-btn mb-3 font-semibold text-[11px] text-surface-300"
    title="Medousa Home"
    onclick={() => onSelect("home")}
  >
    M
  </button>

  <div class="flex flex-1 flex-col gap-0.5">
    {#each items as item (item.id)}
      {@const Icon = item.icon}
      <button
        type="button"
        class="workshop-rail-btn {active === item.id ? 'workshop-rail-btn-active' : ''}"
        title={item.label}
        aria-label={item.label}
        aria-current={active === item.id ? "page" : undefined}
        onclick={() => onSelect(item.id)}
      >
        <Icon {...iconProps} />
      </button>
    {/each}
  </div>

  <button
    type="button"
    class="workshop-rail-btn mt-2 {active === 'settings' ? 'workshop-rail-btn-active' : ''}"
    title="Settings"
    aria-label="Settings"
    aria-current={active === "settings" ? "page" : undefined}
    onclick={() => onSelect("settings")}
  >
    <Settings {...iconProps} />
  </button>
</nav>
