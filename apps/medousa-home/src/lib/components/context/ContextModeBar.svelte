<script lang="ts">
  import { Activity, Brain, Map, MessagesSquare } from "@lucide/svelte";
  import { contextShell } from "$lib/stores/contextShell.svelte";
  import { CONTEXT_TABS, type ContextTabId } from "$lib/types/context";

  const MODES: Array<{
    id: ContextTabId;
    label: string;
    hint: string;
    icon: typeof Brain;
  }> = [
    { id: "recall", label: "Recall", hint: "Facts and people she remembers", icon: Brain },
    { id: "threads", label: "Threads", hint: "Moments from your sessions", icon: MessagesSquare },
    { id: "posture", label: "Posture", hint: "How you showed up", icon: Activity },
    { id: "map", label: "Map", hint: "See how sessions connect", icon: Map },
  ];

  const activeTab = $derived(contextShell.activeTab);

  function available(id: ContextTabId): boolean {
    return CONTEXT_TABS.find((tab) => tab.id === id)?.available ?? false;
  }
</script>

<div
  class="lme-side-mode-bar flex shrink-0 items-center gap-0.5 border-b border-surface-500/25 px-1.5 py-1"
  role="tablist"
  aria-label="Context explorer"
  data-debug-label="context-mode-bar"
>
  <div class="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
    {#each MODES as entry (entry.id)}
      {@const Icon = entry.icon}
      {@const isAvailable = available(entry.id)}
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === entry.id}
        class="lme-side-mode-btn inline-flex size-8 shrink-0 items-center justify-center rounded-md transition-colors
          {activeTab === entry.id
          ? 'bg-surface-700/90 text-surface-50'
          : 'text-surface-400 hover:bg-surface-800/80 hover:text-surface-200'}
          {!isAvailable ? 'opacity-45' : ''}"
        title={isAvailable ? entry.hint : `${entry.hint} — coming soon`}
        aria-label={entry.label}
        disabled={!isAvailable}
        onclick={() => contextShell.setTab(entry.id)}
      >
        <Icon size={15} strokeWidth={1.75} />
      </button>
    {/each}
  </div>
</div>
