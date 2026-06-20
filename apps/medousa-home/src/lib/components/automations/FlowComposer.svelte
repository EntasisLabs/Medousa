<script lang="ts">
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import type { AutomationDeliveryMode } from "$lib/types/recurring";
  import type { FlowComposerDraft, WorkflowStepKind, WorkflowStepSpec } from "$lib/types/workflow";
  import { newStepId } from "$lib/types/workflow";

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

  let addKind = $state<WorkflowStepKind>("prompt");

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
</script>

<div class="space-y-4">
  <label class="cron-field">
    <span class="cron-field-label">Flow name</span>
    <div class="{barClass} cron-field-bar cron-field-bar-compact">
      <input
        class="cron-field-input"
        bind:value={draft.name}
        placeholder="Morning research flow"
        spellcheck="false"
        aria-label="Flow name"
      />
    </div>
  </label>

  <div class="workshop-inset p-3">
    <label class="cron-field">
      <span class="cron-field-label">Describe in plain language</span>
      <div class="{barClass} cron-field-bar">
        <GrowingTextarea
          bind:value={draft.goal}
          placeholder="Every weekday, summarize inbox then post to Slack…"
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
      {flows.planning ? "Planning…" : "Plan from goal"}
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
          <option value="prompt">Prompt</option>
          <option value="grapheme">Grapheme</option>
          <option value="mcp">MCP</option>
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
      <p class="workshop-muted mt-2 text-sm">
        No steps yet. Plan from a goal or add Grapheme, MCP, or Prompt steps.
      </p>
    {:else}
      <ul class="mt-3 space-y-3">
        {#each draft.steps as step, index (step.id)}
          <li class="workshop-inset p-3">
            <div class="flex items-start justify-between gap-2">
              <p class="text-xs font-medium uppercase tracking-wide text-primary-300">
                {step.kind}
                <span class="workshop-faint font-mono normal-case">· {step.id}</span>
              </p>
              <button
                type="button"
                class="workshop-text-action text-xs text-error-400"
                onclick={() => removeStep(index)}
              >
                Remove
              </button>
            </div>

            {#if step.kind === "prompt"}
              <label class="cron-field mt-2">
                <span class="cron-field-label">Prompt</span>
                <div class="{barClass} cron-field-bar">
                  <textarea
                    class="composer-bar-input min-h-[2.25rem]"
                    value={step.user_prompt}
                    oninput={(event) =>
                      updateStep(index, {
                        user_prompt: (event.currentTarget as HTMLTextAreaElement).value,
                      })}
                    placeholder="What should this step do?"
                    aria-label="Prompt step text"
                  ></textarea>
                </div>
              </label>
            {:else if step.kind === "grapheme"}
              <label class="cron-field mt-2">
                <span class="cron-field-label">Grapheme source</span>
                <div class="{barClass} cron-field-bar">
                  <textarea
                    class="composer-bar-input min-h-[2.25rem] font-mono text-xs"
                    value={step.source}
                    oninput={(event) =>
                      updateStep(index, {
                        source: (event.currentTarget as HTMLTextAreaElement).value,
                      })}
                    placeholder={'grapheme.run("module.op", args)'}
                    aria-label="Grapheme source"
                  ></textarea>
                </div>
              </label>
            {:else}
              <div class="mt-2 grid gap-2 sm:grid-cols-2">
                <label class="cron-field">
                  <span class="cron-field-label">MCP server</span>
                  <div class="{barClass} cron-field-bar cron-field-bar-compact">
                    <input
                      class="cron-field-input font-mono text-xs"
                      value={step.server_id}
                      oninput={(event) =>
                        updateStep(index, {
                          server_id: (event.currentTarget as HTMLInputElement).value,
                        })}
                      placeholder="server-id"
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
                      placeholder="tool_name"
                    />
                  </div>
                </label>
              </div>
              <label class="cron-field mt-2">
                <span class="cron-field-label">Args (JSON)</span>
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
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="cron-field-row">
    <label class="cron-field cron-field-grow">
      <span class="cron-field-label">Schedule (optional)</span>
      <div class="{barClass} cron-field-bar cron-field-bar-compact">
        <input
          class="cron-field-input font-mono"
          bind:value={draft.cron_expr}
          placeholder="0 9 * * *"
          spellcheck="false"
          aria-label="Flow cron expression"
        />
      </div>
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
      {flows.scheduling ? "Scheduling…" : "Schedule flow"}
    </button>
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={onCancel}>
      Cancel
    </button>
  </div>

  {#if flows.actionMessage}
    <p class="text-xs text-primary-300">{flows.actionMessage}</p>
  {/if}
</div>
