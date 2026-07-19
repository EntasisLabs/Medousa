<script lang="ts">
  import { Brain, PanelsTopLeft } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import type { PreferredMode } from "$lib/utils/preferredMode";

  /** Recommended default — still one tap to workspace-only. */
  let selected = $state<PreferredMode>("workspace-ai");

  async function continueWithMode() {
    if (!selected) return;
    await wizard.choosePreferredMode(selected);
  }
</script>

<div class="wizard-step">
  <button
    type="button"
    class="workshop-text-action self-start text-sm"
    disabled={wizard.busy}
    onclick={() => void wizard.back()}
  >
    ← Back
  </button>

  <div class="wizard-stagger mt-4">
    <h1 id="product-wizard-title" class="wizard-beat text-2xl font-semibold tracking-tight text-surface-50">
      How do you want to begin?
    </h1>
    <p class="wizard-beat mt-2 text-sm leading-relaxed text-surface-400">
      Start with a brain — or just the workspace.
    </p>

    <div class="wizard-beat mt-7 space-y-3">
      <button
        type="button"
        class="wizard-path-card {selected === 'workspace-ai' ? 'wizard-path-card-active' : ''}"
        disabled={wizard.busy}
        onclick={() => (selected = "workspace-ai")}
      >
        <div class="flex items-start gap-3">
          <Brain class="mt-0.5 h-5 w-5 shrink-0 text-primary-300" aria-hidden="true" />
          <div class="min-w-0">
            <div class="flex flex-wrap items-center gap-2">
              <p class="font-semibold text-surface-50">Workspace with a brain</p>
              <span
                class="rounded-full border border-primary-500/40 bg-primary-500/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-primary-200"
              >
                Recommended
              </span>
            </div>
            <p class="mt-1 text-sm leading-relaxed text-surface-300">
              Notes, files, and decks plus Medousa Agent (private model or bring your API key).
            </p>
          </div>
        </div>
      </button>

      <button
        type="button"
        class="wizard-path-card {selected === 'workspace' ? 'wizard-path-card-active' : ''}"
        disabled={wizard.busy}
        onclick={() => (selected = "workspace")}
      >
        <div class="flex items-start gap-3">
          <PanelsTopLeft class="mt-0.5 h-5 w-5 shrink-0 text-primary-300" aria-hidden="true" />
          <div class="min-w-0">
            <p class="font-semibold text-surface-50">Just the workspace</p>
            <p class="mt-1 text-sm leading-relaxed text-surface-300">
              Notes, files, and decks — add AI when you are ready.
            </p>
          </div>
        </div>
      </button>
    </div>
  </div>

  <div class="mt-auto flex justify-end pt-8">
    <button
      type="button"
      class="btn variant-filled-primary wizard-cta min-h-12 px-10"
      disabled={wizard.busy || !selected}
      onclick={() => void continueWithMode()}
    >
      Continue
    </button>
  </div>
</div>
