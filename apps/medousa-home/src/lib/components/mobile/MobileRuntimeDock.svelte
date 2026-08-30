<script lang="ts">
  import { Activity, ListChecks, Send } from "@lucide/svelte";
  import { haptic } from "$lib/haptics";
  import type { RuntimeTab } from "$lib/types/runtime";

  interface Props {
    activeTab: RuntimeTab;
    onTab: (tab: RuntimeTab) => void;
  }

  let { activeTab, onTab }: Props = $props();

  const tabs = [
    { id: "now", label: "Now", icon: Activity },
    { id: "jobs", label: "Jobs", icon: ListChecks },
    { id: "delivery", label: "Delivery", icon: Send },
  ] as const;

  function select(tab: RuntimeTab) {
    if (tab === activeTab) return;
    haptic("light");
    onTab(tab);
  }
</script>

<nav class="mobile-runtime-dock" aria-label="Workshop views">
  {#each tabs as tab (tab.id)}
    {@const Icon = tab.icon}
    <button
      type="button"
      class="mobile-runtime-dock-btn"
      class:mobile-runtime-dock-btn-active={activeTab === tab.id}
      aria-current={activeTab === tab.id ? "page" : undefined}
      aria-label={tab.label}
      onclick={() => select(tab.id)}
    >
      <Icon size={18} strokeWidth={1.75} />
      <span>{tab.label}</span>
    </button>
  {/each}
</nav>

<style>
  .mobile-runtime-dock {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    flex-shrink: 0;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-900) / 0.92);
  }

  .mobile-runtime-dock-btn {
    display: flex;
    min-width: 0;
    min-height: 2.75rem;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.125rem;
    padding: 0.35rem 0.25rem;
    color: rgb(var(--color-content-quiet));
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  .mobile-runtime-dock-btn-active {
    color: rgb(var(--color-content-link));
  }
</style>
