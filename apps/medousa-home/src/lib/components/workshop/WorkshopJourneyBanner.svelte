<script lang="ts">
  import { dismissWorkshopJourney, isWorkshopJourneyDismissed } from "$lib/config/workshopGuidance";
  import { flows } from "$lib/stores/flows.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { settings } from "$lib/stores/settings.svelte";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  let dismissed = $state(isWorkshopJourneyDismissed());
  let open = $state(false);

  const activeStep = $derived.by(() => {
    const tab = graphemeScriptEditor.activeTab;
    const hasScript = Boolean(tab?.body.trim());
    const hasFlowDraft =
      flows.composerOpen && flows.composerDraft.steps.length > 0;
    const hasSchedule = Boolean(
      flows.composerOpen && flows.composerDraft.cron_expr.trim(),
    );

    if (hasSchedule || (hasFlowDraft && flows.composerDraft.name.trim())) {
      return 3;
    }
    if (hasFlowDraft || (hasScript && tab?.scriptId)) {
      return 2;
    }
    if (hasScript) {
      return 2;
    }
    return 1;
  });

  const steps = [
    { n: 1, label: "Write", hint: "Script or recipe" },
    { n: 2, label: "Flow", hint: "Add to flow" },
    { n: 3, label: "Schedule", hint: "Cron rhythm" },
  ] as const;

  $effect(() => {
    if (dismissed || !settings.showWorkshopGuidance) return;
    if (activeStep >= 2) open = true;
  });

  function hideJourney() {
    dismissWorkshopJourney();
    dismissed = true;
    open = false;
  }
</script>

{#if settings.showWorkshopGuidance && !dismissed}
  <details class="workshop-journey-details {compact ? 'workshop-journey-details-compact' : ''}" bind:open>
    <summary class="workshop-journey-summary">
      <span>Write → Flow → Schedule</span>
      {#if activeStep > 1}
        <span class="workshop-faint text-[10px]">· step {activeStep} of 3</span>
      {/if}
    </summary>
    <div class="workshop-journey {compact ? 'workshop-journey-compact' : ''}">
      <ol class="workshop-journey-steps">
        {#each steps as step (step.n)}
          <li
            class="workshop-journey-step {activeStep === step.n
              ? 'workshop-journey-step-active'
              : activeStep > step.n
                ? 'workshop-journey-step-done'
                : ''}"
          >
            <span class="workshop-journey-badge">{step.n}</span>
            <span class="min-w-0">
              <span class="block text-xs font-medium text-surface-100">{step.label}</span>
              {#if !compact}
                <span class="workshop-faint block text-[10px]">{step.hint}</span>
              {/if}
            </span>
          </li>
        {/each}
      </ol>
      <button
        type="button"
        class="workshop-text-action mt-2 text-[10px]"
        onclick={hideJourney}
      >
        Hide path
      </button>
    </div>
  </details>
{/if}
