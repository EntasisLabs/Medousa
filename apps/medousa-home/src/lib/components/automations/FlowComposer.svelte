<script lang="ts">
  import FriendlySchedulePicker from "$lib/components/automations/FriendlySchedulePicker.svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import { getGraphemeScript } from "$lib/daemon";
  import { flows } from "$lib/stores/flows.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { AutomationDeliveryMode } from "$lib/types/recurring";
  import type { FlowComposerDraft, WorkflowStepKind, WorkflowStepSpec } from "$lib/types/workflow";
  import { newStepId } from "$lib/types/workflow";
  import {
    flowStepSequenceLabel,
    flowStepSubtitle,
    GRAPHEME_STEP_PLACEHOLDER,
  } from "$lib/utils/flowStepLabels";
  import "./flowComposer.css";

  interface Props {
    mobile?: boolean;
    draft: FlowComposerDraft;
    deliveryMode?: AutomationDeliveryMode;
    telegramChatId?: string;
    onCancel: () => void;
  }

  let {
    mobile = false,
    draft = $bindable(),
    deliveryMode = "in_app",
    telegramChatId = "",
    onCancel,
  }: Props = $props();

  let addKind = $state<WorkflowStepKind>("grapheme");
  let expandedStepIds = $state<Record<string, boolean>>({});
  let libraryLoadBusy = $state<string | null>(null);
  let planOpen = $state(false);
  let scheduleOpen = $state(false);

  $effect(() => {
    if (workshop.scripts.length === 0) {
      void workshop.refreshModulesAndScripts();
    }
  });

  const barClass = $derived(mobile ? "composer-bar composer-bar-mobile" : "composer-bar");

  function addStep(kind: WorkflowStepKind) {
    const id = newStepId(kind.slice(0, 3));
    const step: WorkflowStepSpec =
      kind === "grapheme"
        ? { kind: "grapheme", id, source: "" }
        : kind === "mcp"
          ? { kind: "mcp", id, server_id: "", tool_name: "", args: {} }
          : { kind: "prompt", id, user_prompt: "" };
    draft = { ...draft, steps: [...draft.steps, step] };
    expandedStepIds = {
      ...expandedStepIds,
      [id]: kind === "grapheme" ? false : !mobile,
    };
  }

  function removeStep(index: number) {
    draft = { ...draft, steps: draft.steps.filter((_, i) => i !== index) };
  }

  function updateStep(index: number, patch: Partial<WorkflowStepSpec>) {
    draft = {
      ...draft,
      steps: draft.steps.map((step, i) =>
        i === index ? ({ ...step, ...patch } as WorkflowStepSpec) : step,
      ),
    };
  }

  function toggleStepEditor(stepId: string) {
    expandedStepIds = {
      ...expandedStepIds,
      [stepId]: !expandedStepIds[stepId],
    };
  }

  function isStepExpanded(stepId: string): boolean {
    return expandedStepIds[stepId] ?? false;
  }

  async function loadLibraryScript(stepIndex: number, scriptId: string) {
    if (!scriptId.trim()) return;
    const step = draft.steps[stepIndex];
    if (!step || step.kind !== "grapheme") return;
    libraryLoadBusy = step.id;
    try {
      const detail = await getGraphemeScript(scriptId);
      const entry = workshop.scripts.find((row) => row.id === scriptId);
      updateStep(stepIndex, {
        source: detail.body_preview,
        script_id: scriptId,
        script_name: entry?.name ?? detail.script.name,
      });
      expandedStepIds = { ...expandedStepIds, [step.id]: false };
    } finally {
      libraryLoadBusy = null;
    }
  }

  function openSchedulePath() {
    scheduleOpen = true;
  }

  async function confirmSchedule() {
    try {
      await flows.scheduleDraft(draft, deliveryMode, telegramChatId);
    } catch (err) {
      flows.actionMessage = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<div class="flow-composer flow-liquid {mobile ? 'flow-composer-mobile' : ''} space-y-6">
  <div class="flow-liquid-beat">
    <label class="block">
      <span class="flow-liquid-whisper">Flow name</span>
      <input
        class="flow-liquid-name"
        bind:value={draft.name}
        placeholder="Morning web digest"
        spellcheck="false"
        aria-label="Flow name"
      />
    </label>
  </div>

  <div class="flow-liquid-beat">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <p class="flow-liquid-whisper">Steps</p>
      <div class="flex flex-wrap items-center gap-2">
        <select
          class="rounded-md border-0 bg-surface-800/60 px-2 py-1 text-xs text-surface-300 outline-none ring-1 ring-inset ring-surface-500/25 focus:ring-primary-500/40"
          bind:value={addKind}
          aria-label="Step type to add"
        >
          <option value="grapheme">Script</option>
          <option value="prompt">Ask Medousa</option>
          <option value="mcp">External tool</option>
        </select>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => addStep(addKind)}
        >
          + Add step
        </button>
      </div>
    </div>

    {#if draft.steps.length === 0}
      <p class="flow-spine-empty">Add a step, or choose a recipe.</p>
    {:else}
      <ol class="flow-spine">
        {#each draft.steps as step, index (step.id)}
          <li class="flow-spine-item">
            <div class="flow-spine-rail" aria-hidden="true">
              <span class="flow-spine-node"></span>
              <span class="flow-spine-connector"></span>
            </div>
            <div class="flow-spine-body">
              <div class="flex items-start justify-between gap-2">
                <div class="min-w-0">
                  <p class="text-[10px] font-semibold uppercase tracking-wide text-primary-400/90">
                    {index === 0 ? "Start" : `Then`}
                  </p>
                  <p class="mt-0.5 text-sm font-medium text-surface-50">
                    {flowStepSequenceLabel(index, step)}
                  </p>
                  <p class="mt-0.5 text-[11px] text-surface-500">{flowStepSubtitle(step)}</p>
                </div>
                <button
                  type="button"
                  class="workshop-text-action shrink-0 text-xs text-surface-500 hover:text-error-400"
                  onclick={() => removeStep(index)}
                >
                  Remove
                </button>
              </div>

              {#if step.kind === "prompt"}
                <label class="cron-field mt-3 block">
                  <span class="sr-only">Instructions</span>
                  <div class="{barClass} cron-field-bar">
                    <textarea
                      class="composer-bar-input min-h-[2.25rem]"
                      value={step.user_prompt}
                      oninput={(event) =>
                        updateStep(index, {
                          user_prompt: (event.currentTarget as HTMLTextAreaElement).value,
                        })}
                      placeholder="Summarize the top headlines and email me a bullet list"
                      aria-label="Prompt step text"
                    ></textarea>
                  </div>
                </label>
              {:else if step.kind === "grapheme"}
                {#if !isStepExpanded(step.id)}
                  <button
                    type="button"
                    class="workshop-text-action mt-2 text-[11px]"
                    onclick={() => toggleStepEditor(step.id)}
                  >
                    {step.source.trim() ? "Edit script" : "Add script"}
                  </button>
                {:else}
                  {#if workshop.scripts.length > 0}
                    <label class="cron-field mt-3 block">
                      <span class="cron-field-label">From script library</span>
                      <select
                        class="cron-field-input w-full rounded-md border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
                        value={step.script_id ?? ""}
                        disabled={libraryLoadBusy === step.id}
                        onchange={(event) =>
                          void loadLibraryScript(
                            index,
                            (event.currentTarget as HTMLSelectElement).value,
                          )}
                        aria-label="Load saved script"
                      >
                        <option value="">Pick saved script…</option>
                        {#each workshop.scripts as entry (entry.id)}
                          <option value={entry.id}>{entry.name}</option>
                        {/each}
                      </select>
                    </label>
                  {/if}
                  <label class="cron-field mt-2 block">
                    <span class="cron-field-label">Grapheme source</span>
                    <div class="{barClass} cron-field-bar">
                      <textarea
                        class="composer-bar-input min-h-[5rem] font-mono text-xs"
                        value={step.source}
                        oninput={(event) =>
                          updateStep(index, {
                            source: (event.currentTarget as HTMLTextAreaElement).value,
                          })}
                        placeholder={GRAPHEME_STEP_PLACEHOLDER}
                        aria-label="Grapheme source"
                      ></textarea>
                    </div>
                  </label>
                  <button
                    type="button"
                    class="workshop-text-action mt-2 text-[11px]"
                    onclick={() => toggleStepEditor(step.id)}
                  >
                    Hide script
                  </button>
                {/if}
              {:else if step.kind === "mcp"}
                <details class="workshop-advanced mt-3">
                  <summary class="workshop-text-action cursor-pointer text-[11px]">
                    Configure external tool
                  </summary>
                  <div class="mt-2 grid gap-2 sm:grid-cols-2">
                    <label class="cron-field">
                      <span class="cron-field-label">Server</span>
                      <div class="{barClass} cron-field-bar cron-field-bar-compact">
                        <input
                          class="cron-field-input font-mono text-xs"
                          value={step.server_id}
                          oninput={(event) =>
                            updateStep(index, {
                              server_id: (event.currentTarget as HTMLInputElement).value,
                            })}
                          placeholder="my-mcp-server"
                        />
                      </div>
                    </label>
                    <label class="cron-field">
                      <span class="cron-field-label">Tool</span>
                      <div class="{barClass} cron-field-bar cron-field-bar-compact">
                        <input
                          class="cron-field-input font-mono text-xs"
                          value={step.tool_name}
                          oninput={(event) =>
                            updateStep(index, {
                              tool_name: (event.currentTarget as HTMLInputElement).value,
                            })}
                          placeholder="search"
                        />
                      </div>
                    </label>
                  </div>
                  <label class="cron-field mt-2 block">
                    <span class="cron-field-label">Arguments (JSON)</span>
                    <div class="{barClass} cron-field-bar">
                      <textarea
                        class="composer-bar-input min-h-[2.25rem] font-mono text-xs"
                        value={JSON.stringify(step.args ?? {}, null, 2)}
                        oninput={(event) => {
                          try {
                            const parsed = JSON.parse(
                              (event.currentTarget as HTMLTextAreaElement).value || "{}",
                            );
                            updateStep(index, { args: parsed });
                          } catch {
                            /* keep typing */
                          }
                        }}
                        aria-label="MCP args JSON"
                      ></textarea>
                    </div>
                  </label>
                </details>
              {/if}
            </div>
          </li>
        {/each}
      </ol>
    {/if}
  </div>

  <div class="flow-liquid-beat flow-liquid-optional">
    <div class="flow-liquid-disclosure">
      <button
        type="button"
        class="flow-liquid-disclosure-btn"
        onclick={() => (planOpen = !planOpen)}
        aria-expanded={planOpen}
      >
        <span>
          <span class="flow-liquid-disclosure-title">Plan with AI</span>
          <span class="mt-0.5 block text-[11px] text-surface-500">
            Generate steps from a goal
          </span>
        </span>
        <span class="text-surface-500">{planOpen ? "−" : "+"}</span>
      </button>
      {#if planOpen}
        <div class="pb-3 pl-0.5">
          <label class="cron-field">
            <span class="cron-field-label">Goal</span>
            <div class="{barClass} cron-field-bar">
              <GrowingTextarea
                bind:value={draft.goal}
                placeholder="Every weekday, search for news and summarize it for me…"
                minHeight={mobile ? 34 : 36}
                maxHeight={mobile ? 120 : 96}
                aria-label="Flow goal for planning"
              />
            </div>
          </label>
          <button
            type="button"
            class="btn btn-sm variant-soft-primary mt-2"
            disabled={flows.planning || !draft.goal.trim()}
            onclick={() => void flows.planFromGoal(draft.goal)}
          >
            {flows.planning ? "Planning…" : "Plan steps"}
          </button>
          {#if flows.lastPlan?.notes.length}
            <ul class="workshop-faint mt-2 list-disc space-y-1 pl-4 text-[11px]">
              {#each flows.lastPlan.notes.slice(0, 3) as note (note)}
                <li>{note}</li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>

    <div class="flow-liquid-disclosure">
      <button
        type="button"
        class="flow-liquid-disclosure-btn"
        onclick={() => (scheduleOpen = !scheduleOpen)}
        aria-expanded={scheduleOpen}
      >
        <span>
          <span class="flow-liquid-disclosure-title">Schedule</span>
          <span class="mt-0.5 block text-[11px] text-surface-500">
            Run on a schedule
          </span>
        </span>
        <span class="text-surface-500">{scheduleOpen ? "−" : "+"}</span>
      </button>
      {#if scheduleOpen}
        <div class="pb-3 pl-0.5">
          <FriendlySchedulePicker
            {mobile}
            optional
            bind:cronExpr={draft.cron_expr}
            bind:timezone={draft.timezone}
            label="When it repeats"
          />
          <button
            type="button"
            class="btn btn-sm variant-soft-primary mt-3"
            disabled={flows.scheduling || draft.steps.length === 0 || !draft.cron_expr.trim()}
            onclick={() => void confirmSchedule()}
          >
            {flows.scheduling ? "Scheduling…" : "Schedule"}
          </button>
        </div>
      {/if}
    </div>
  </div>

  <div
    class="flow-liquid-beat flow-composer-actions flex flex-wrap items-center gap-2 {mobile
      ? 'flow-composer-actions-mobile'
      : ''}"
  >
    <button
      type="button"
      class="flow-liquid-cta flow-liquid-cta-primary btn btn-sm variant-filled-primary"
      disabled={flows.running || draft.steps.length === 0}
      onclick={() => void flows.runDraft(draft)}
    >
      {flows.running ? "Running…" : "Run now"}
    </button>
    <button
      type="button"
      class="flow-liquid-cta btn btn-sm variant-ghost-surface"
      disabled={draft.steps.length === 0}
      onclick={openSchedulePath}
    >
      Schedule…
    </button>
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={onCancel}>
      Cancel
    </button>
  </div>

  {#if flows.actionMessage}
    <p class="text-xs text-primary-300">{flows.actionMessage}</p>
  {/if}
</div>
