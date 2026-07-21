<script lang="ts">
  import AgentEditorTitlebar from "$lib/components/skills/AgentEditorTitlebar.svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import type { ManuscriptCatalogEntry } from "$lib/types/catalog";
  import type { UpdateManuscriptRequest } from "$lib/types/manuscript";
  import { workshopMonogram } from "$lib/types/workshopRegistry";
  import {
    displayVoiceAppendix,
    humanizeScheduleValidationError,
    isSkillYamlResidue,
  } from "$lib/utils/agentVoiceField";
  import "./agentEditor.css";

  interface Props {
    entry: ManuscriptCatalogEntry;
    onRunSkill: (manuscriptId: string) => void;
    onUseInAutomation: (entry: ManuscriptCatalogEntry) => void;
    onScheduleSkill: (entry: ManuscriptCatalogEntry) => void;
    onOpenFile: (path: string) => void;
    /** Hide shell rail expand (SkillsPanel / mobile). */
    hideSidebarExpand?: boolean;
  }

  let {
    entry,
    onRunSkill,
    onUseInAutomation,
    onScheduleSkill,
    onOpenFile,
    hideSidebarExpand = false,
  }: Props = $props();

  let name = $state("");
  let description = $state("");
  let displayName = $state("");
  let voiceAppendix = $state("");
  /** True when disk voice looked like SKILL/YAML residue — Save clears it unless user types prose. */
  let voiceWasResidue = $state(false);
  let taskTemplate = $state("");
  let scheduleCron = $state("");
  let scheduleExecutionMode = $state("agent_turn");
  let deliveryMode = $state("");
  let deliveryOnComplete = $state("");
  let toolsAllow = $state<string[]>([]);
  let openshellAllowScheduled = $state(false);

  const detail = $derived(catalog.manuscriptDetail);

  const scheduleErrorHuman = $derived(
    humanizeScheduleValidationError(detail?.schedule_validation_error),
  );

  const monogram = $derived(workshopMonogram(name.trim() || entry.name || "Agent"));

  $effect(() => {
    if (!detail || detail.id !== entry.id) return;
    name = detail.name ?? entry.name;
    description = detail.description ?? entry.description ?? "";
    displayName = detail.display_name ?? "";
    const rawVoice = detail.voice_appendix ?? "";
    voiceWasResidue = isSkillYamlResidue(rawVoice);
    voiceAppendix = displayVoiceAppendix(rawVoice);
    taskTemplate = detail.task_template ?? "";
    scheduleCron = detail.schedule_cron ?? "";
    scheduleExecutionMode = detail.schedule_execution_mode ?? "agent_turn";
    deliveryMode = detail.delivery_mode ?? "";
    deliveryOnComplete = detail.delivery_on_complete ?? "";
    toolsAllow = [...detail.tools_allow];
    openshellAllowScheduled = detail.openshell.allow_scheduled;
  });

  function toggleTool(tool: string, enabled: boolean) {
    if (enabled) {
      if (!toolsAllow.includes(tool)) {
        toolsAllow = [...toolsAllow, tool];
      }
    } else {
      toolsAllow = toolsAllow.filter((value) => value !== tool);
    }
  }

  async function saveChanges() {
    const voiceTrimmed = voiceAppendix.trim();
    const request: UpdateManuscriptRequest = {
      name: name.trim() || undefined,
      description: description.trim() || undefined,
      clear_description: description.trim() ? undefined : true,
      display_name: displayName.trim() || undefined,
      clear_display_name: displayName.trim() ? undefined : true,
      // Empty field clears disk residue (including SKILL YAML dumps) on save.
      voice_appendix: voiceTrimmed || undefined,
      clear_voice_appendix: voiceTrimmed ? undefined : true,
      task_template: taskTemplate.trim() || undefined,
      clear_task_template: taskTemplate.trim() ? undefined : true,
      tools_allow: toolsAllow,
      schedule_cron: scheduleCron.trim() || undefined,
      clear_schedule_cron: scheduleCron.trim() ? undefined : true,
      schedule_execution_mode: scheduleExecutionMode.trim() || undefined,
      delivery_mode: deliveryMode.trim() || undefined,
      delivery_on_complete: deliveryOnComplete.trim() || undefined,
      openshell_allow_scheduled: openshellAllowScheduled,
    };
    await catalog.saveManuscriptDetail(entry.id, request);
    if (voiceTrimmed || voiceWasResidue) voiceWasResidue = false;
  }
</script>

<span class="sr-only">Agent id {entry.id}</span>

{#if catalog.manuscriptDetailLoading}
  <p class="workshop-muted mt-4 px-5 text-sm">Loading editor…</p>
{:else if catalog.manuscriptDetailError}
  <p class="mt-4 px-5 text-sm text-warning-400">{catalog.manuscriptDetailError}</p>
{:else if detail}
  <div class="agent-liquid flex h-full min-h-0 flex-col">
    <AgentEditorTitlebar
      {hideSidebarExpand}
      saveBusy={catalog.manuscriptSaveBusy}
      saveDisabled={!name.trim()}
      onSave={() => void saveChanges()}
      onRun={() => onRunSkill(entry.id)}
      palette={detail.palette_tools}
      {toolsAllow}
      onToggleTool={toggleTool}
      openshellEnabled={detail.openshell.enabled}
      openshellDefaultPath={detail.openshell.default_path}
      bind:openshellAllowScheduled
      scripts={entry.scripts}
      scheduleReady={detail.schedule_ready}
      scheduleErrorHuman={scheduleErrorHuman}
      bind:scheduleCron
      bind:scheduleExecutionMode
      bind:deliveryMode
      bind:deliveryOnComplete
      scheduledTools={detail.scheduled_tools}
      onScheduleSkill={() => onScheduleSkill(entry)}
      onUseInAutomation={() => onUseInAutomation(entry)}
    />

    <section
      class="agent-liquid-identity min-h-0 flex-1 overflow-y-auto px-5 py-5 sm:px-7 sm:py-6"
      aria-label="Specialist"
    >
      <div class="agent-liquid-form">
        <div class="agent-liquid-field flex items-center gap-3">
          <div class="agent-liquid-mono" aria-hidden="true">{monogram}</div>
          <div class="min-w-0 flex-1">
            <p class="agent-liquid-whisper">Specialist name</p>
            <input
              class="agent-liquid-name"
              bind:value={name}
              placeholder="Name"
              aria-label="Specialist name"
            />
          </div>
        </div>

        <div class="agent-liquid-field mt-6">
          <p class="agent-liquid-whisper">Role</p>
          <GrowingTextarea
            class="agent-liquid-textarea mt-1"
            bind:value={description}
            minHeight={48}
            maxHeight={120}
            placeholder="What this specialist does…"
            aria-label="Role"
          />
        </div>

        <div class="agent-liquid-field mt-5">
          <p class="agent-liquid-whisper">Display name</p>
          <input
            class="agent-liquid-input mt-1"
            bind:value={displayName}
            placeholder="How it appears in conversation…"
            aria-label="Display name"
          />
        </div>

        <div class="agent-liquid-field mt-5">
          <p class="agent-liquid-whisper">Tone</p>
          <GrowingTextarea
            class="agent-liquid-textarea mt-1"
            bind:value={voiceAppendix}
            minHeight={56}
            maxHeight={140}
            placeholder="Voice and style in plain language…"
            aria-label="Tone"
          />
        </div>

        <div class="agent-liquid-field mt-5">
          <p class="agent-liquid-whisper">When invoked</p>
          <GrowingTextarea
            class="agent-liquid-textarea mt-1"
            bind:value={taskTemplate}
            minHeight={56}
            maxHeight={140}
            placeholder="Instructions for each run…"
            aria-label="When invoked"
          />
        </div>

        {#if catalog.manuscriptSaveMessage}
          <p class="mt-5 text-xs text-surface-400">{catalog.manuscriptSaveMessage}</p>
        {/if}

        <div class="agent-liquid-foot mt-8 flex flex-wrap gap-3">
          <button
            type="button"
            class="workshop-text-action text-xs"
            onclick={() => onOpenFile(entry.path)}
          >
            Open YAML
          </button>
          <span class="workshop-faint self-center font-mono text-[10px]">{entry.id}</span>
        </div>
      </div>
    </section>
  </div>
{/if}
