<script lang="ts">
  import { AUTOMATIONS_SECTIONS } from "$lib/automationsSections";
  import { haptic } from "$lib/haptics";
  import type { AutomationsSection } from "$lib/stores/automationsNav.svelte";

  interface Props {
    section: AutomationsSection;
    onSection: (section: AutomationsSection) => void;
  }

  let { section, onSection }: Props = $props();

  function select(next: AutomationsSection) {
    if (next === section) return;
    haptic("light");
    onSection(next);
  }
</script>

<nav class="mobile-automations-dock" aria-label="Automations views">
  {#each AUTOMATIONS_SECTIONS as tab (tab.id)}
    {@const Icon = tab.icon}
    <button
      type="button"
      class="mobile-automations-dock-btn"
      class:mobile-automations-dock-btn-active={section === tab.id}
      aria-current={section === tab.id ? "page" : undefined}
      aria-label={tab.label}
      onclick={() => select(tab.id)}
    >
      <Icon size={18} strokeWidth={1.75} />
      <span>{tab.label}</span>
    </button>
  {/each}
</nav>

<style>
  .mobile-automations-dock {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    flex-shrink: 0;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-900) / 0.92);
  }

  .mobile-automations-dock-btn {
    display: flex;
    min-width: 0;
    min-height: 2.75rem;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.125rem;
    padding: 0.35rem 0.125rem;
    overflow: hidden;
    color: rgb(var(--color-content-quiet));
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.01em;
  }

  .mobile-automations-dock-btn span {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mobile-automations-dock-btn-active {
    color: rgb(var(--color-content-link));
  }
</style>
