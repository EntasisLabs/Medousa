<script lang="ts">
  import FlowEditorTitlebar from "$lib/components/automations/FlowEditorTitlebar.svelte";
  import { getGraphemeScript } from "$lib/daemon";
  import { flows } from "$lib/stores/flows.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { AutomationDeliveryMode } from "$lib/types/recurring";
  import type { FlowComposerDraft, WorkflowStepSpec } from "$lib/types/workflow";
  import {
    flowStepSequenceLabel,
    flowStepSubtitle,
    GRAPHEME_STEP_PLACEHOLDER,
  } from "$lib/utils/flowStepLabels";
  import { X } from "@lucide/svelte";
  import "./flowComposer.css";

  interface Props {
    mobile?: boolean;
    draft: FlowComposerDraft;
    deliveryMode?: AutomationDeliveryMode;
    telegramChatId?: string;
    onCancel: () => void;
    /** Hide shell rail expand (FlowsPanel / mobile). */
    hideSidebarExpand?: boolean;
  }

  let {
    mobile = false,
    draft = $bindable(),
    deliveryMode = "in_app",
    telegramChatId = "",
    onCancel,
    hideSidebarExpand = false,
  }: Props = $props();

  let expandedStepIds = $state<Record<string, boolean>>({});
  let libraryLoadBusy = $state<string | null>(null);

  $effect(() => {
    if (workshop.scripts.length === 0) {
      void workshop.refreshModulesAndScripts();
    }
  });

  function addStep(step: WorkflowStepSpec) {
    draft = { ...draft, steps: [...draft.steps, step] };
    // New steps arrive complete from the add popover — keep collapsed.
    expandedStepIds = { ...expandedStepIds, [step.id]: false };
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

  function scriptPreview(source: string): string {
    const lines = source.trim().split("\n").slice(0, 6);
    if (lines.length === 0) return "";
    const body = lines.join("\n");
    const total = source.trim().split("\n").length;
    return total > 6 ? `${body}\n…` : body;
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
    } finally {
      libraryLoadBusy = null;
    }
  }

  async function confirmSchedule() {
    try {
      await flows.scheduleDraft(draft, deliveryMode, telegramChatId);
    } catch (err) {
      flows.actionMessage = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<div class="flow-composer flex h-full min-h-0 flex-col {mobile ? 'flow-composer-mobile' : ''}">
  <FlowEditorTitlebar
    {hideSidebarExpand}
    {mobile}
    bind:goal={draft.goal}
    bind:cronExpr={draft.cron_expr}
    bind:timezone={draft.timezone}
    stepCount={draft.steps.length}
    onRun={() => void flows.runDraft(draft)}
    {onCancel}
    onConfirmSchedule={confirmSchedule}
    onAddStep={addStep}
  />

  <div class="flow-composer-body min-h-0 flex-1 overflow-y-auto px-5 py-5 sm:px-7 sm:py-6">
    <div class="flow-composer-form mx-auto w-full max-w-2xl space-y-6">
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

      <div>
        <p class="flow-liquid-whisper">Steps</p>

        {#if draft.steps.length === 0}
          <p class="flow-spine-empty">Add a step from the toolbar, or choose a recipe.</p>
        {:else}
          <ol class="flow-spine">
            {#each draft.steps as step, index (step.id)}
              <li class="flow-spine-item group/step">
                <div class="flow-spine-rail" aria-hidden="true">
                  <span class="flow-spine-node"></span>
                  <span class="flow-spine-connector"></span>
                </div>
                <div class="flow-spine-body">
                  <div class="flex items-start justify-between gap-2">
                    <div class="min-w-0">
                      <p class="text-sm font-medium text-surface-50">
                        {flowStepSequenceLabel(index, step)}
                      </p>
                      <p class="mt-0.5 text-[11px] text-surface-500">{flowStepSubtitle(step)}</p>
                    </div>
                    <button
                      type="button"
                      class="flow-spine-remove shrink-0"
                      aria-label="Remove step"
                      title="Remove step"
                      onclick={() => removeStep(index)}
                    >
                      <X size={12} strokeWidth={2} />
                    </button>
                  </div>

                  {#if step.kind === "prompt"}
                    {#if isStepExpanded(step.id)}
                      <textarea
                        class="flow-prompt-editor mt-2"
                        value={step.user_prompt}
                        oninput={(event) =>
                          updateStep(index, {
                            user_prompt: (event.currentTarget as HTMLTextAreaElement).value,
                          })}
                        placeholder="What should Medousa do?"
                        aria-label="Prompt step text"
                        rows={3}
                      ></textarea>
                      <button
                        type="button"
                        class="workshop-text-action mt-1.5 text-[11px]"
                        onclick={() => toggleStepEditor(step.id)}
                      >
                        Done
                      </button>
                    {:else}
                      <button
                        type="button"
                        class="flow-prompt-preview mt-2"
                        onclick={() => toggleStepEditor(step.id)}
                      >
                        {step.user_prompt.trim() || "Empty prompt — click to edit"}
                      </button>
                    {/if}
                  {:else if step.kind === "grapheme"}
                    {#if isStepExpanded(step.id)}
                      <div class="flow-script-panel mt-2">
                        {#if workshop.scripts.length > 0}
                          <div class="flow-script-panel-toolbar">
                            <select
                              class="flow-script-library-select"
                              value={step.script_id ?? ""}
                              disabled={libraryLoadBusy === step.id}
                              onchange={(event) =>
                                void loadLibraryScript(
                                  index,
                                  (event.currentTarget as HTMLSelectElement).value,
                                )}
                              aria-label="Load saved script"
                            >
                              <option value="">Library…</option>
                              {#each workshop.scripts as entry (entry.id)}
                                <option value={entry.id}>{entry.name}</option>
                              {/each}
                            </select>
                            <button
                              type="button"
                              class="workshop-text-action text-[11px]"
                              onclick={() => toggleStepEditor(step.id)}
                            >
                              Done
                            </button>
                          </div>
                        {:else}
                          <div class="flow-script-panel-toolbar justify-end">
                            <button
                              type="button"
                              class="workshop-text-action text-[11px]"
                              onclick={() => toggleStepEditor(step.id)}
                            >
                              Done
                            </button>
                          </div>
                        {/if}
                        <textarea
                          class="flow-script-editor"
                          value={step.source}
                          oninput={(event) =>
                            updateStep(index, {
                              source: (event.currentTarget as HTMLTextAreaElement).value,
                            })}
                          placeholder={GRAPHEME_STEP_PLACEHOLDER}
                          aria-label="Grapheme source"
                          rows={10}
                        ></textarea>
                      </div>
                    {:else}
                      <button
                        type="button"
                        class="flow-script-preview mt-2"
                        onclick={() => toggleStepEditor(step.id)}
                        title="Edit script"
                      >
                        {#if step.source.trim()}
                          <pre class="flow-script-preview-code">{scriptPreview(step.source)}</pre>
                        {:else}
                          <span class="text-[11px] text-surface-500">No script — click to edit</span>
                        {/if}
                      </button>
                    {/if}
                  {:else if step.kind === "mcp"}
                    {#if isStepExpanded(step.id)}
                      <div class="mt-2 grid gap-2 sm:grid-cols-2">
                        <label class="block">
                          <span class="flow-liquid-whisper">Server</span>
                          <input
                            class="mt-1 w-full rounded-md border border-surface-500/35 bg-surface-900 px-2 py-1.5 font-mono text-xs"
                            value={step.server_id}
                            oninput={(event) =>
                              updateStep(index, {
                                server_id: (event.currentTarget as HTMLInputElement).value,
                              })}
                            placeholder="my-mcp-server"
                          />
                        </label>
                        <label class="block">
                          <span class="flow-liquid-whisper">Tool</span>
                          <input
                            class="mt-1 w-full rounded-md border border-surface-500/35 bg-surface-900 px-2 py-1.5 font-mono text-xs"
                            value={step.tool_name}
                            oninput={(event) =>
                              updateStep(index, {
                                tool_name: (event.currentTarget as HTMLInputElement).value,
                              })}
                            placeholder="search"
                          />
                        </label>
                      </div>
                      <label class="mt-2 block">
                        <span class="flow-liquid-whisper">Arguments</span>
                        <textarea
                          class="flow-script-editor mt-1"
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
                          rows={3}
                        ></textarea>
                      </label>
                      <button
                        type="button"
                        class="workshop-text-action mt-1.5 text-[11px]"
                        onclick={() => toggleStepEditor(step.id)}
                      >
                        Done
                      </button>
                    {:else}
                      <button
                        type="button"
                        class="flow-prompt-preview mt-2 font-mono"
                        onclick={() => toggleStepEditor(step.id)}
                      >
                        {step.server_id || "server"}.{step.tool_name || "tool"}
                      </button>
                    {/if}
                  {/if}
                </div>
              </li>
            {/each}
          </ol>
        {/if}
      </div>

      {#if flows.actionMessage}
        <p class="text-xs text-primary-300">{flows.actionMessage}</p>
      {/if}
    </div>
  </div>
</div>
