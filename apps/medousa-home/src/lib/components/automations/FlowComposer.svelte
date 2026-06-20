<script lang="ts">
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import GraphemeRecipeCards from "$lib/components/grapheme/GraphemeRecipeCards.svelte";
  import WorkshopJourneyBanner from "$lib/components/workshop/WorkshopJourneyBanner.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import type { GraphemeRecipe } from "$lib/grapheme/graphemeRecipes";
  import type { AutomationDeliveryMode } from "$lib/types/recurring";
  import type { FlowComposerDraft, WorkflowStepKind, WorkflowStepSpec } from "$lib/types/workflow";
  import { newStepId } from "$lib/types/workflow";
  import {
    flowStepSubtitle,
    flowStepTitle,
    GRAPHEME_STEP_PLACEHOLDER,
  } from "$lib/utils/flowStepLabels";

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
    expandedStepIds = { ...expandedStepIds, [id]: kind !== "grapheme" };
  }

  function addRecipe(recipe: GraphemeRecipe) {
    const id = newStepId("gph");
    draft = {
      ...draft,
      name: draft.name.trim() || recipe.flowName || recipe.scriptName,
      goal: draft.goal.trim() || (settings.showWorkshopGuidance ? recipe.intent : ""),
      steps: [
        ...draft.steps,
        { kind: "grapheme", id, source: recipe.body },
      ],
    };
    expandedStepIds = { ...expandedStepIds, [id]: true };
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
</script>

<div class="space-y-4">
  {#if settings.showWorkshopGuidance}
    <WorkshopJourneyBanner compact />
  {/if}

  <label class="cron-field">
    <span class="cron-field-label">Flow name</span>
    <div class="{barClass} cron-field-bar cron-field-bar-compact">
      <input
        class="cron-field-input"
        bind:value={draft.name}
        placeholder="Morning web digest"
        spellcheck="false"
        aria-label="Flow name"
      />
    </div>
  </label>

  <div class="workshop-inset p-3">
    <label class="cron-field">
      <span class="cron-field-label">Goal (optional — for AI planning)</span>
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

  <div>
    <div class="flex flex-wrap items-center justify-between gap-2">
      <p class="workshop-label">Steps</p>
      <div class="flex flex-wrap items-center gap-2">
        <select
          class="cron-field-input rounded-md border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
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
      {#if settings.showWorkshopGuidance}
        <GraphemeRecipeCards
          compact
          title="Starter recipes"
          hint="Adds a grapheme step — or use + Add step."
          onselect={addRecipe}
        />
      {:else}
        <p class="workshop-muted mt-3 text-sm">Add a step to begin.</p>
      {/if}
    {:else}
      <ol class="mt-3 space-y-3">
        {#each draft.steps as step, index (step.id)}
          <li class="workshop-flow-step">
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <p class="text-[10px] font-semibold uppercase tracking-wide text-primary-400">
                  Step {index + 1}
                </p>
                <p class="mt-1 text-sm font-medium text-surface-50">
                  {flowStepTitle(step)}
                </p>
                <p class="workshop-faint mt-0.5 text-[11px]">{flowStepSubtitle(step)}</p>
              </div>
              <button
                type="button"
                class="workshop-text-action shrink-0 text-xs text-error-400"
                onclick={() => removeStep(index)}
              >
                Remove
              </button>
            </div>

            {#if step.kind === "prompt"}
              <label class="cron-field mt-3 block">
                <span class="cron-field-label">Instructions</span>
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
              {#if step.source.trim()}
                <label class="cron-field mt-3 block">
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
              {:else}
                <button
                  type="button"
                  class="workshop-text-action mt-3 text-[11px]"
                  onclick={() => toggleStepEditor(step.id)}
                >
                  {isStepExpanded(step.id) ? "Hide script" : "Add script"}
                </button>
                {#if isStepExpanded(step.id)}
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
                {/if}
              {/if}
            {:else}
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
          </li>
        {/each}
      </ol>
    {/if}
  </div>

  <div class="cron-field-row">
    <label class="cron-field cron-field-grow">
      <span class="cron-field-label">Run on a schedule (optional)</span>
      <div class="{barClass} cron-field-bar cron-field-bar-compact">
        <input
          class="cron-field-input font-mono"
          bind:value={draft.cron_expr}
          placeholder="0 9 * * *  weekdays 9am"
          spellcheck="false"
          aria-label="Flow cron expression"
        />
      </div>
      <p class="workshop-faint mt-1 text-[10px]">Cron format · e.g. 0 9 * * 1-5 for weekdays</p>
    </label>
    <label class="cron-field cron-field-timezone">
      <span class="cron-field-label">Timezone</span>
      <div class="{barClass} cron-field-bar cron-field-bar-compact">
        <input
          class="cron-field-input font-mono"
          bind:value={draft.timezone}
          placeholder="UTC"
          spellcheck="false"
          aria-label="Flow timezone"
        />
      </div>
    </label>
  </div>

  <div class="flex flex-wrap gap-2 pt-1">
    <button
      type="button"
      class="btn btn-sm variant-filled-primary"
      disabled={flows.running || draft.steps.length === 0}
      onclick={() => void flows.runDraft(draft)}
    >
      {flows.running ? "Running…" : "Run now"}
    </button>
    <button
      type="button"
      class="btn btn-sm variant-soft-primary"
      disabled={flows.scheduling || draft.steps.length === 0}
      onclick={() => void flows.scheduleDraft(draft, deliveryMode, telegramChatId)}
    >
      {flows.scheduling ? "Scheduling…" : "Schedule"}
    </button>
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={onCancel}>
      Cancel
    </button>
  </div>

  {#if flows.actionMessage}
    <p class="text-xs text-primary-300">{flows.actionMessage}</p>
  {/if}
</div>
