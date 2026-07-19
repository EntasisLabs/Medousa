<script lang="ts">
  import AgentToolsPicker from "$lib/components/skills/AgentToolsPicker.svelte";
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
  }

  let {
    entry,
    onRunSkill,
    onUseInAutomation,
    onScheduleSkill,
    onOpenFile,
  }: Props = $props();

  let rightPane = $state<"tools" | "schedule">("tools");
  let showOpenshell = $state(false);
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
    rightPane = "tools";
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

  function openToolsFromSchedule() {
    rightPane = "tools";
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
  <p class="workshop-muted mt-4 text-sm">Loading editor…</p>
{:else if catalog.manuscriptDetailError}
  <p class="mt-4 text-sm text-warning-400">{catalog.manuscriptDetailError}</p>
{:else if detail}
  <div class="agent-liquid h-full min-h-0">
    <div class="agent-liquid-wash" aria-hidden="true"></div>

    <div class="agent-liquid-split">
      <section class="agent-liquid-identity" aria-label="Who she is">
        <div class="agent-liquid-field flex items-center gap-3.5">
          <div class="agent-liquid-mono" aria-hidden="true">{monogram}</div>
          <div class="min-w-0 flex-1">
            <p class="text-[11px] tracking-[0.14em] text-surface-500 uppercase">Meet her</p>
            <input
              class="agent-liquid-name"
              bind:value={name}
              placeholder="Name her…"
              aria-label="Name"
            />
          </div>
        </div>

        <div class="agent-liquid-field mt-6">
          <p class="agent-liquid-whisper">What she helps with</p>
          <GrowingTextarea
            class="agent-liquid-textarea mt-1"
            bind:value={description}
            minHeight={48}
            maxHeight={120}
            placeholder="A short job — enough that you’d trust her with it…"
            aria-label="What she helps with"
          />
        </div>

        <div class="agent-liquid-field mt-5">
          <p class="agent-liquid-whisper">How she introduces herself</p>
          <input
            class="agent-liquid-input mt-1"
            bind:value={displayName}
            placeholder="What you call her in conversation…"
            aria-label="How she introduces herself"
          />
        </div>

        <div class="agent-liquid-field mt-5">
          <p class="agent-liquid-whisper">How she sounds</p>
          <GrowingTextarea
            class="agent-liquid-textarea mt-1"
            bind:value={voiceAppendix}
            minHeight={56}
            maxHeight={140}
            placeholder="Tone in plain language — warm, sharp, spare…"
            aria-label="How she sounds"
          />
        </div>

        <div class="agent-liquid-field mt-5">
          <p class="agent-liquid-whisper">When you call on her</p>
          <GrowingTextarea
            class="agent-liquid-textarea mt-1"
            bind:value={taskTemplate}
            minHeight={56}
            maxHeight={140}
            placeholder="What she does the moment she’s invoked…"
            aria-label="When you call on her"
          />
        </div>

        <div class="agent-liquid-field mt-7 flex flex-wrap items-center gap-3">
          <button
            type="button"
            class="agent-liquid-cta btn btn-sm variant-filled-primary"
            disabled={catalog.manuscriptSaveBusy || !name.trim()}
            onclick={() => void saveChanges()}
          >
            {catalog.manuscriptSaveBusy ? "Saving…" : "This feels right"}
          </button>
          <button
            type="button"
            class="agent-liquid-cta btn btn-sm variant-ghost-surface"
            onclick={() => onRunSkill(entry.id)}
          >
            Run in chat
          </button>
          {#if catalog.manuscriptSaveMessage}
            <span class="text-xs text-surface-400">{catalog.manuscriptSaveMessage}</span>
          {/if}
        </div>

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
      </section>

      <aside class="agent-liquid-powers" aria-label="Powers and schedule">
        <div class="agent-liquid-powers-head">
          <div>
            <p class="text-[11px] tracking-[0.12em] text-surface-500 uppercase">Alongside her</p>
            <p class="mt-1 text-sm text-surface-300">
              {#if rightPane === "tools"}
                {#if toolsAllow.length > 0}
                  {toolsAllow.length} tools ready
                {:else}
                  Choose what she may reach for
                {/if}
              {:else}
                Recurring runs — only if you want them
              {/if}
            </p>
          </div>
          <div class="agent-liquid-pane-tabs" role="tablist" aria-label="Powers pane">
            <button
              type="button"
              role="tab"
              aria-selected={rightPane === "tools"}
              class="agent-liquid-pane-tab {rightPane === 'tools'
                ? 'agent-liquid-pane-tab-active'
                : ''}"
              onclick={() => (rightPane = "tools")}
            >
              Tools
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={rightPane === "schedule"}
              class="agent-liquid-pane-tab {rightPane === 'schedule'
                ? 'agent-liquid-pane-tab-active'
                : ''}"
              onclick={() => (rightPane = "schedule")}
            >
              Schedule
            </button>
          </div>
        </div>

        <div class="agent-liquid-powers-body" data-pane={rightPane}>
          {#if rightPane === "tools"}
            <AgentToolsPicker
              palette={detail.palette_tools}
              selected={toolsAllow}
              onToggle={toggleTool}
              fill
            />

            {#if detail.openshell.enabled}
              <div class="mt-3 shrink-0 rounded-lg border border-surface-500/25 bg-surface-950/30">
                <button
                  type="button"
                  class="flex w-full items-center justify-between px-3 py-2 text-left text-xs"
                  onclick={() => (showOpenshell = !showOpenshell)}
                >
                  <span class="text-surface-300">OpenShell sandbox</span>
                  <span class="text-surface-500">{showOpenshell ? "−" : "+"}</span>
                </button>
                {#if showOpenshell}
                  <div class="border-t border-surface-500/25 px-3 py-2.5 text-xs text-surface-300">
                    <p>
                      Default path:
                      <span class="text-surface-200">{detail.openshell.default_path}</span>
                    </p>
                    <label class="mt-3 flex items-center gap-2">
                      <input
                        type="checkbox"
                        class="checkbox"
                        bind:checked={openshellAllowScheduled}
                      />
                      Allow OpenShell tools on scheduled runs
                    </label>
                  </div>
                {/if}
              </div>
            {/if}

            {#if entry.scripts.length > 0}
              <div class="mt-3 shrink-0">
                <p class="agent-liquid-whisper">Scripts</p>
                <ul class="mt-2 space-y-1 text-[11px] text-surface-300">
                  {#each entry.scripts as script (script.relative_path)}
                    <li class="font-mono">
                      {script.relative_path}
                      <span class="text-surface-500">({script.risk_class})</span>
                    </li>
                  {/each}
                </ul>
              </div>
            {/if}
          {:else}
            <div class="rounded-lg border border-surface-500/25 bg-surface-950/30 px-3 py-2.5 text-xs">
              <div class="flex flex-wrap items-center gap-2">
                <span class="text-surface-400">Schedule readiness</span>
                {#if detail.schedule_ready}
                  <span class="text-[10px] tracking-wide text-primary-300 uppercase">Ready</span>
                {:else}
                  <span class="text-[10px] tracking-wide text-warning-400 uppercase">Needs a step</span>
                {/if}
              </div>
              {#if scheduleErrorHuman}
                <p class="mt-1 text-warning-400/90">{scheduleErrorHuman}</p>
                {#if /tool/i.test(scheduleErrorHuman)}
                  <button
                    type="button"
                    class="workshop-text-action mt-2 text-xs"
                    onclick={openToolsFromSchedule}
                  >
                    Choose tools →
                  </button>
                {/if}
              {/if}
            </div>

            <div class="mt-4 grid gap-4">
              <label class="block">
                <span class="agent-liquid-whisper">Default schedule</span>
                <input
                  class="agent-liquid-input mt-1 font-mono"
                  bind:value={scheduleCron}
                  placeholder="0 9 * * *"
                  aria-label="Default schedule cron"
                />
              </label>
              <label class="block">
                <span class="agent-liquid-whisper">How it runs</span>
                <select
                  class="agent-liquid-input mt-1"
                  bind:value={scheduleExecutionMode}
                  aria-label="How it runs"
                >
                  <option value="agent_turn">Full agent turn</option>
                  <option value="prompt">Quick prompt only</option>
                </select>
              </label>
              <label class="block">
                <span class="agent-liquid-whisper">Delivery</span>
                <input
                  class="agent-liquid-input mt-1"
                  bind:value={deliveryMode}
                  placeholder="optional"
                  aria-label="Delivery"
                />
              </label>
              <label class="block">
                <span class="agent-liquid-whisper">On complete</span>
                <select
                  class="agent-liquid-input mt-1"
                  bind:value={deliveryOnComplete}
                  aria-label="On complete"
                >
                  <option value="">None</option>
                  <option value="locus">Remember (Locus)</option>
                  <option value="store">Store</option>
                </select>
              </label>
            </div>

            {#if detail.scheduled_tools.length > 0}
              <div class="mt-4">
                <p class="agent-liquid-whisper">Scheduled lane preview</p>
                <ul class="mt-2 max-h-36 space-y-1 overflow-y-auto">
                  {#each detail.scheduled_tools as row (row.tool)}
                    <li class="rounded-lg border border-surface-500/25 px-2.5 py-1.5 text-[11px]">
                      <div class="flex flex-wrap items-center gap-2">
                        <span class="font-mono text-surface-200">{row.tool}</span>
                        <span
                          class="text-[10px] tracking-wide uppercase {row.allowed_on_schedule
                            ? 'text-primary-300'
                            : 'text-warning-400/80'}"
                        >
                          {row.allowed_on_schedule ? "schedule ok" : "interactive only"}
                        </span>
                      </div>
                      {#if row.reason}
                        <p class="workshop-faint mt-0.5">{row.reason}</p>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </div>
            {/if}

            <div class="mt-4 flex flex-wrap gap-3">
              <button
                type="button"
                class="workshop-text-action text-xs"
                onclick={() => onScheduleSkill(entry)}
              >
                Schedule…
              </button>
              <button
                type="button"
                class="workshop-text-action text-xs"
                onclick={() => onUseInAutomation(entry)}
              >
                Use in automation
              </button>
            </div>
          {/if}
        </div>
      </aside>
    </div>
  </div>
{/if}
