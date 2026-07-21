<script lang="ts">
  import { MessageSquare, Save } from "@lucide/svelte";
  import AgentSchedulePopover from "$lib/components/skills/AgentSchedulePopover.svelte";
  import AgentToolsPopover from "$lib/components/skills/AgentToolsPopover.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import type { ManuscriptScriptEntry } from "$lib/types/catalog";
  import type { ManuscriptScheduledToolEntry } from "$lib/types/manuscript";

  interface Props {
    /** Hide rail expand (e.g. SkillsPanel mobile chrome). */
    hideSidebarExpand?: boolean;
    saveBusy: boolean;
    saveDisabled: boolean;
    onSave: () => void;
    onRun: () => void;
    palette: string[];
    toolsAllow: string[];
    onToggleTool: (toolId: string, enabled: boolean) => void;
    openshellEnabled?: boolean;
    openshellDefaultPath?: string;
    openshellAllowScheduled?: boolean;
    scripts?: ManuscriptScriptEntry[];
    scheduleReady: boolean;
    scheduleErrorHuman: string;
    scheduleCron?: string;
    scheduleExecutionMode?: string;
    deliveryMode?: string;
    deliveryOnComplete?: string;
    scheduledTools?: ManuscriptScheduledToolEntry[];
    onScheduleSkill?: () => void;
    onUseInAutomation?: () => void;
  }

  let {
    hideSidebarExpand = false,
    saveBusy,
    saveDisabled,
    onSave,
    onRun,
    palette,
    toolsAllow,
    onToggleTool,
    openshellEnabled = false,
    openshellDefaultPath = "",
    openshellAllowScheduled = $bindable(false),
    scripts = [],
    scheduleReady,
    scheduleErrorHuman,
    scheduleCron = $bindable(""),
    scheduleExecutionMode = $bindable("agent_turn"),
    deliveryMode = $bindable(""),
    deliveryOnComplete = $bindable(""),
    scheduledTools = [],
    onScheduleSkill,
    onUseInAutomation,
  }: Props = $props();

  let toolsOpen = $state(false);
  let scheduleOpen = $state(false);

  function openToolsFromSchedule() {
    scheduleOpen = false;
    toolsOpen = true;
  }
</script>

<div
  class="scripts-workbench-titlebar relative z-40 flex shrink-0 items-center gap-1 border-b border-surface-500/35 px-1 py-0.5"
>
  {#if !hideSidebarExpand}
    <ShellSidebarExpandButton label="Show workspace browser" />
  {/if}

  <div class="min-w-0 flex-1" aria-hidden="true"></div>

  <div
    class="scripts-workbench-titlebar-actions ml-auto flex shrink-0 items-center gap-0.5 pl-1"
  >
    <button
      type="button"
      class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
      title={saveBusy ? "Saving…" : "Save"}
      aria-label="Save specialist"
      disabled={saveBusy || saveDisabled}
      onclick={onSave}
    >
      <Save size={15} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-run"
      title="Run in chat"
      aria-label="Run in chat"
      onclick={onRun}
    >
      <MessageSquare size={15} strokeWidth={1.75} />
    </button>

    <span class="mx-0.5 h-4 w-px shrink-0 bg-surface-500/40" aria-hidden="true"></span>

    <AgentToolsPopover
      bind:open={toolsOpen}
      {palette}
      selected={toolsAllow}
      onToggle={onToggleTool}
      {openshellEnabled}
      {openshellDefaultPath}
      bind:openshellAllowScheduled
      {scripts}
      onOpenChange={(next) => {
        if (next) scheduleOpen = false;
      }}
    />
    <AgentSchedulePopover
      bind:open={scheduleOpen}
      {scheduleReady}
      {scheduleErrorHuman}
      bind:scheduleCron
      bind:scheduleExecutionMode
      bind:deliveryMode
      bind:deliveryOnComplete
      {scheduledTools}
      onChooseTools={openToolsFromSchedule}
      {onScheduleSkill}
      {onUseInAutomation}
      onOpenChange={(next) => {
        if (next) toolsOpen = false;
      }}
    />
  </div>
</div>
