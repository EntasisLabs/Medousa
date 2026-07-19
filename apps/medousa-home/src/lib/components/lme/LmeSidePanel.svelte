<script lang="ts">
  import {
    Blocks,
    BookOpen,
    Bot,
    CalendarClock,
    FileCode2,
    Files,
    GitBranch,
    History,
    LayoutTemplate,
    Package,
    Presentation,
  } from "@lucide/svelte";
  import LmeAgentsExplorer from "$lib/components/lme/explorers/LmeAgentsExplorer.svelte";
  import LmeAutomationsExplorer from "$lib/components/lme/explorers/LmeAutomationsExplorer.svelte";
  import LmeDecksExplorer from "$lib/components/lme/explorers/LmeDecksExplorer.svelte";
  import LmeFilesExplorer from "$lib/components/lme/explorers/LmeFilesExplorer.svelte";
  import LmeNotesExplorer from "$lib/components/lme/explorers/LmeNotesExplorer.svelte";
  import LmeScriptsExplorer from "$lib/components/lme/explorers/LmeScriptsExplorer.svelte";
  import {
    lmeWorkspace,
    type LmeExplorerMode,
    type LmeScriptsExplorerSection,
  } from "$lib/stores/lmeWorkspace.svelte";

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
    { id: "files", label: "Files", icon: Files },
    { id: "presentations", label: "Decks", icon: Presentation },
    { id: "scripts", label: "Scripts", icon: FileCode2 },
    { id: "agents", label: "Agents", icon: Bot },
    { id: "flows", label: "Flows", icon: GitBranch },
    { id: "schedules", label: "Schedules", icon: CalendarClock },
    { id: "history", label: "History", icon: History },
  ];

  const SCRIPT_SECTIONS: Array<{
    id: LmeScriptsExplorerSection;
    label: string;
    icon: typeof FileCode2;
  }> = [
    { id: "scripts", label: "Library", icon: FileCode2 },
    { id: "templates", label: "Templates", icon: LayoutTemplate },
    { id: "modules", label: "Modules", icon: Blocks },
    { id: "wasm", label: "WASM", icon: Package },
  ];

  const mode = $derived(lmeWorkspace.explorerMode);
  const modeLabel = $derived(MODES.find((entry) => entry.id === mode)?.label ?? "Workspace");
</script>

<div
  class="lme-side-panel flex h-full min-h-0 w-full flex-col border-r border-surface-500/45 bg-surface-900/40"
  data-debug-label="lme-side-panel"
>
  <div
    class="lme-side-mode-bar flex shrink-0 items-center gap-0.5 overflow-x-auto border-b border-surface-500/40 px-1.5 py-1"
    role="tablist"
    aria-label="Workspace explorer"
  >
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

  {#if mode === "scripts"}
    <div
      class="lme-scripts-subnav flex shrink-0 items-center gap-0.5 overflow-x-auto border-b border-surface-500/35 px-1.5 py-1"
      role="tablist"
      aria-label="Scripts explorer sections"
    >
      {#each SCRIPT_SECTIONS as entry (entry.id)}
        {@const Icon = entry.icon}
        <button
          type="button"
          role="tab"
          aria-selected={lmeWorkspace.scriptsExplorerSection === entry.id}
          class="inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-[10px] font-medium transition-colors
            {lmeWorkspace.scriptsExplorerSection === entry.id
            ? 'bg-surface-800 text-surface-100'
            : 'text-surface-500 hover:bg-surface-800/60 hover:text-surface-300'}"
          title={entry.label}
          onclick={() => lmeWorkspace.setScriptsExplorerSection(entry.id)}
        >
          <Icon size={12} strokeWidth={1.75} />
          <span>{entry.label}</span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="flex min-h-0 items-center justify-between border-b border-surface-500/30 px-3 py-1.5">
    <p class="workshop-label text-[10px]">{modeLabel}</p>
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
    {:else}
      <LmeAutomationsExplorer />
    {/if}
  </div>
</div>
