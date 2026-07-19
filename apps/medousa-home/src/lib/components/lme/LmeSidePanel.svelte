<script lang="ts">
  import {
    BookOpen,
    Bot,
    CalendarClock,
    FileCode2,
    Files,
    GitBranch,
    History,
    PanelLeftClose,
    Presentation,
  } from "@lucide/svelte";
  import LmeAgentsExplorer from "$lib/components/lme/explorers/LmeAgentsExplorer.svelte";
  import LmeAutomationsExplorer from "$lib/components/lme/explorers/LmeAutomationsExplorer.svelte";
  import LmeFlowsExplorer from "$lib/components/lme/explorers/LmeFlowsExplorer.svelte";
  import LmeDecksExplorer from "$lib/components/lme/explorers/LmeDecksExplorer.svelte";
  import LmeFilesExplorer from "$lib/components/lme/explorers/LmeFilesExplorer.svelte";
  import LmeNotesExplorer from "$lib/components/lme/explorers/LmeNotesExplorer.svelte";
  import LmeScriptsExplorer from "$lib/components/lme/explorers/LmeScriptsExplorer.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace, type LmeExplorerMode } from "$lib/stores/lmeWorkspace.svelte";

  interface Props {
    onOpenChat: () => void;
  }

  let { onOpenChat }: Props = $props();

  const MODES: Array<{
    id: LmeExplorerMode;
    label: string;
    icon: typeof BookOpen;
  }> = [
    { id: "notes", label: "Notes", icon: BookOpen },
    { id: "files", label: "Local Files", icon: Files },
    { id: "presentations", label: "Presentations", icon: Presentation },
    { id: "scripts", label: "Scripts", icon: FileCode2 },
    { id: "agents", label: "Agents", icon: Bot },
    { id: "flows", label: "Flows", icon: GitBranch },
    { id: "schedules", label: "Schedules", icon: CalendarClock },
    { id: "history", label: "History", icon: History },
  ];

  const mode = $derived(lmeWorkspace.explorerMode);
</script>

<div
  class="lme-side-panel flex h-full min-h-0 w-full flex-col border-r border-surface-500/30 bg-surface-900/30"
  data-debug-label="lme-side-panel"
>
  <div
    class="lme-side-mode-bar flex shrink-0 items-center gap-0.5 border-b border-surface-500/25 px-1.5 py-1"
    role="tablist"
    aria-label="Workspace explorer"
  >
    <div class="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
      {#each MODES as entry (entry.id)}
        {@const Icon = entry.icon}
        <button
          type="button"
          role="tab"
          aria-selected={mode === entry.id}
          class="lme-side-mode-btn inline-flex size-8 shrink-0 items-center justify-center rounded-md transition-colors
            {mode === entry.id
            ? 'bg-surface-700/90 text-surface-50'
            : 'text-surface-400 hover:bg-surface-800/80 hover:text-surface-200'}"
          title={entry.label}
          aria-label={entry.label}
          onclick={() => lmeWorkspace.setExplorerMode(entry.id)}
        >
          <Icon size={15} strokeWidth={1.75} />
        </button>
      {/each}
    </div>
    <button
      type="button"
      class="lme-side-mode-btn inline-flex size-8 shrink-0 items-center justify-center rounded-md text-surface-400 transition-colors hover:bg-surface-800/80 hover:text-surface-200"
      title="Hide workspace browser"
      aria-label="Hide workspace browser"
      onclick={() => layout.setVaultSidebarCollapsed(true)}
    >
      <PanelLeftClose size={15} strokeWidth={1.75} />
    </button>
  </div>

  <div class="min-h-0 flex-1 overflow-hidden">
    {#if mode === "notes"}
      <LmeNotesExplorer />
    {:else if mode === "files"}
      <LmeFilesExplorer />
    {:else if mode === "presentations"}
      <LmeDecksExplorer {onOpenChat} />
    {:else if mode === "scripts"}
      <LmeScriptsExplorer />
    {:else if mode === "agents"}
      <LmeAgentsExplorer {onOpenChat} />
    {:else if mode === "flows"}
      <LmeFlowsExplorer />
    {:else}
      <LmeAutomationsExplorer />
    {/if}
  </div>
</div>
