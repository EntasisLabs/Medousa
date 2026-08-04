<script lang="ts">
  import { onMount } from "svelte";
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import SettingsPresentationRetention from "$lib/components/settings/SettingsPresentationRetention.svelte";
  import SettingsStorageGovernance from "$lib/components/settings/SettingsStorageGovernance.svelte";
  import ModelsSettingsTab from "$lib/components/settings/ModelsSettingsTab.svelte";
  import ModelsStagesTab from "$lib/components/settings/ModelsStagesTab.svelte";
  import ProvidersSettingsTab from "$lib/components/settings/ProvidersSettingsTab.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import {
    BUILTIN_VOICE_PRESETS,
    MAX_CUSTOM_VOICE_PRESETS,
    normalizeCustomVoicePresets,
    resolveVoicePreset,
    uniqueVoicePresetId,
    type VoicePreset,
  } from "$lib/types/voicePresets";
  import { depthModeLabel } from "$lib/utils/chatModelPicker";
  import { formatModelDisplayName } from "$lib/utils/formatModelDisplay";
  import { listProviders, type ProvidersListResult } from "$lib/utils/providersApi";
  import { composerSttStatus } from "$lib/utils/composerStt";
  import { openGuide } from "$lib/guide/openGuide";
  import {
    getAgentModeTransitionPolicy,
    setAgentModeTransitionPolicy,
  } from "$lib/daemon";
  import type {
    AgentModeAutoAccept,
    AgentModeTransitionPolicy,
  } from "$lib/types/generated/daemon_api";
  import { ChevronDown } from "@lucide/svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  type ModelsExtra = "stages" | "providers" | null;
  type Picker = "stance" | "depth" | null;

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const memoryPrimary = [
    {
      key: "sliceHotWindowTurns" as const,
      label: "Hot memory",
      hint: "Recent turns in the prompt",
      unit: "turns",
      min: 2,
      max: 32,
    },
    {
      key: "sliceColdWindowTurns" as const,
      label: "Cold recall",
      hint: "Older turns still reachable",
      unit: "turns",
      min: 4,
      max: 64,
    },
  ];

  const memoryBudgets = [
    {
      key: "activationLongSessionTurnThreshold" as const,
      label: "Long chat after",
      hint: "When long-thread rules kick in",
      unit: "turns",
      min: 8,
      max: 80,
    },
    {
      key: "activationDirectAnswerMaxPromptChars" as const,
      label: "Direct-answer budget",
      hint: "Max prompt for a quick answer",
      unit: "chars",
      min: 200,
      max: 20000,
      step: 20,
      wide: true,
    },
    {
      key: "activationLongSessionMaxPromptChars" as const,
      label: "Long-chat budget",
      hint: "Max prompt once a thread is long",
      unit: "chars",
      min: 200,
      max: 20000,
      step: 20,
      wide: true,
    },
  ];

  let catalog = $state<ProvidersListResult | null>(null);
  let sttReady = $state(false);
  let modelsTab: ModelsSettingsTab | undefined = $state();
  let picker = $state<Picker>(null);
  let modelsExtra = $state<ModelsExtra>(null);
  let modelsAdvancedOpen = $state(false);
  let memoryOpen = $state(false);
  let memoryBudgetsOpen = $state(false);
  let voicesOpen = $state(false);
  let presentationsOpen = $state(false);
  let storageOpen = $state(false);
  let modeTransitionsOpen = $state(false);
  let modePolicy = $state<AgentModeTransitionPolicy>({
    proposal_ttl_seconds: 30,
    auto_accept: "never",
  });
  let modePolicySaving = $state(false);
  let modePolicyFeedback = $state<string | null>(null);

  let editorOpen = $state(false);
  let editingId = $state<string | null>(null);
  let draftName = $state("");
  let draftDescription = $state("");
  let draftAppendix = $state("");

  const customPresets = $derived(
    normalizeCustomVoicePresets(workshopDefaults.draft.customVoicePresets),
  );
  const allPresets = $derived([...BUILTIN_VOICE_PRESETS, ...customPresets]);
  const activeVoice = $derived(
    resolveVoicePreset(workshopDefaults.draft.activeVoiceId, customPresets),
  );
  const activeDepth = $derived(
    DEPTH_CHARTER_OPTIONS.find(
      (option) => option.id === (workshopDefaults.draft.responseDepthMode ?? "standard"),
    ) ?? DEPTH_CHARTER_OPTIONS[1]!,
  );
  const canAddCustom = $derived(customPresets.length < MAX_CUSTOM_VOICE_PRESETS);

  const agentSummary = $derived.by(() => {
    const hot = workshopDefaults.draft.sliceHotWindowTurns ?? "—";
    const cold = workshopDefaults.draft.sliceColdWindowTurns ?? "—";
    const model = formatModelDisplayName(workshopDefaults.draft.model ?? "model");
    return `${activeVoice.name} · ${depthModeLabel(activeDepth.id)} · ${model} · ${hot}/${cold} turns`;
  });

  const memorySummary = $derived.by(() => {
    const hot = workshopDefaults.draft.sliceHotWindowTurns ?? "—";
    const cold = workshopDefaults.draft.sliceColdWindowTurns ?? "—";
    return `${hot} hot · ${cold} cold`;
  });

  onMount(() => {
    void bootstrap();
  });

  async function bootstrap() {
    try {
      catalog = await listProviders();
    } catch {
      catalog = null;
    }
    try {
      const stt = await composerSttStatus();
      sttReady = stt.available;
    } catch {
      sttReady = false;
    }
    try {
      modePolicy = await getAgentModeTransitionPolicy();
    } catch {
      modePolicyFeedback = "Mode policy is unavailable on this workshop.";
    }
  }

  async function saveModePolicy(next: AgentModeTransitionPolicy) {
    if (readOnly || modePolicySaving) return;
    modePolicySaving = true;
    modePolicyFeedback = null;
    try {
      modePolicy = await setAgentModeTransitionPolicy(next);
      modePolicyFeedback = "Saved";
    } catch (err) {
      modePolicyFeedback = err instanceof Error ? err.message : String(err);
    } finally {
      modePolicySaving = false;
    }
  }

  async function refreshSttAndKeys() {
    const stt = await composerSttStatus();
    sttReady = stt.available;
    await modelsTab?.refreshKeyStatus();
  }

  function togglePicker(next: Picker) {
    picker = picker === next ? null : next;
  }

  function toggleModelsExtra(next: Exclude<ModelsExtra, null>) {
    modelsExtra = modelsExtra === next ? null : next;
    if (modelsExtra) modelsAdvancedOpen = true;
  }

  function numField(
    key: (typeof memoryPrimary)[number]["key"] | (typeof memoryBudgets)[number]["key"],
    event: Event,
  ) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: Number.isFinite(value) ? value : null,
    };
  }

  function resetEditor() {
    editorOpen = false;
    editingId = null;
    draftName = "";
    draftDescription = "";
    draftAppendix = "";
  }

  function openCreateEditor() {
    if (readOnly || !canAddCustom) return;
    editingId = null;
    draftName = "";
    draftDescription = "";
    draftAppendix = "";
    editorOpen = true;
    voicesOpen = true;
  }

  function openEditEditor(preset: VoicePreset) {
    if (readOnly || preset.builtin) return;
    editingId = preset.id;
    draftName = preset.name;
    draftDescription = preset.description ?? "";
    draftAppendix = preset.voiceAppendix;
    editorOpen = true;
    voicesOpen = true;
  }

  function setActiveVoice(voiceId: string) {
    if (readOnly || workshopDefaults.saving) return;
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      activeVoiceId: voiceId,
    };
    voicePresets.applyFromDraft(workshopDefaults.draft);
    picker = null;
  }

  function setDepth(mode: (typeof DEPTH_CHARTER_OPTIONS)[number]["id"]) {
    if (readOnly || workshopDefaults.saving) return;
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      responseDepthMode: mode,
    };
    picker = null;
  }

  function deleteCustomPreset(voiceId: string) {
    if (readOnly || workshopDefaults.saving) return;
    const nextCustom = customPresets.filter((preset) => preset.id !== voiceId);
    const nextActive = activeVoice.id === voiceId ? "default" : activeVoice.id;
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      customVoicePresets: nextCustom,
      activeVoiceId: nextActive,
    };
    voicePresets.applyFromDraft(workshopDefaults.draft);
    if (editingId === voiceId) resetEditor();
  }

  function saveEditor() {
    if (readOnly || workshopDefaults.saving) return;
    const name = draftName.trim();
    const voiceAppendix = draftAppendix.trim();
    if (!name || !voiceAppendix) return;

    let nextCustom = [...customPresets];
    if (editingId) {
      nextCustom = nextCustom.map((preset) =>
        preset.id === editingId
          ? {
              ...preset,
              name,
              description: draftDescription.trim() || undefined,
              voiceAppendix,
            }
          : preset,
      );
    } else {
      const ids = new Set(nextCustom.map((preset) => preset.id));
      const id = uniqueVoicePresetId(name, ids);
      nextCustom = [
        ...nextCustom,
        {
          id,
          name,
          description: draftDescription.trim() || undefined,
          voiceAppendix,
        },
      ].slice(0, MAX_CUSTOM_VOICE_PRESETS);
    }

    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      customVoicePresets: nextCustom,
    };
    voicePresets.applyFromDraft(workshopDefaults.draft);
    resetEditor();
  }
</script>

<section class="settings-section prefs agent">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Medousa Agent</h2>
    <p class="workshop-faint mt-1 text-sm">{agentSummary}</p>
  </header>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Answers</h3>
      <p class="settings-subsection-lead">Stance and how deep she goes.</p>
    </div>

    <div class="prefs-stack">
      <div class="agent-active">
        <button
          type="button"
          class="agent-active-trigger"
          class:agent-active-trigger-open={picker === "stance"}
          aria-expanded={picker === "stance"}
          disabled={readOnly || workshopDefaults.saving}
          onclick={() => togglePicker("stance")}
        >
          <span class="agent-active-copy">
            <span class="agent-active-kicker">Stance</span>
            <span class="agent-active-title">{activeVoice.name}</span>
            {#if activeVoice.description}
              <span class="agent-active-meta">{activeVoice.description}</span>
            {/if}
          </span>
          <span class="agent-active-action workshop-faint">
            {picker === "stance" ? "Close" : "Change"}
          </span>
        </button>

        {#if picker === "stance"}
          <div class="prefs-grid agent-picker" role="listbox" aria-label="Choose stance">
            {#each allPresets as preset (preset.id)}
              <button
                type="button"
                role="option"
                class="prefs-choice"
                class:prefs-choice-active={activeVoice.id === preset.id}
                aria-selected={activeVoice.id === preset.id}
                disabled={readOnly || workshopDefaults.saving}
                title={preset.description}
                onclick={() => setActiveVoice(preset.id)}
              >
                <span class="prefs-choice-label">{preset.name}</span>
                {#if preset.description}
                  <span class="prefs-choice-hint">{preset.description}</span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <div class="agent-active">
        <button
          type="button"
          class="agent-active-trigger"
          class:agent-active-trigger-open={picker === "depth"}
          aria-expanded={picker === "depth"}
          disabled={readOnly || workshopDefaults.saving}
          onclick={() => togglePicker("depth")}
        >
          <span class="agent-active-copy">
            <span class="agent-active-kicker">Depth</span>
            <span class="agent-active-title">{activeDepth.label}</span>
            <span class="agent-active-meta">{activeDepth.hint}</span>
          </span>
          <span class="agent-active-action workshop-faint">
            {picker === "depth" ? "Close" : "Change"}
          </span>
        </button>

        {#if picker === "depth"}
          <div class="prefs-grid agent-picker" role="listbox" aria-label="Choose answer depth">
            {#each DEPTH_CHARTER_OPTIONS as option (option.id)}
              <button
                type="button"
                role="option"
                class="prefs-choice"
                class:prefs-choice-active={activeDepth.id === option.id}
                aria-selected={activeDepth.id === option.id}
                disabled={readOnly || workshopDefaults.saving}
                title={option.hint}
                onclick={() => setDepth(option.id)}
              >
                <span class="prefs-choice-label">{option.label}</span>
                <span class="prefs-choice-hint">{option.hint}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Models</h3>
      <p class="settings-subsection-lead">
        Who answers, sees, and listens — changes apply immediately.
      </p>
      <button
        type="button"
        class="settings-learn-more"
        onclick={() => void openGuide("chat")}
      >
        Learn more
      </button>
    </div>

    <div class="agent-models">
      <ModelsSettingsTab
        bind:this={modelsTab}
        {catalog}
        {sttReady}
        disabled={readOnly || workshopDefaults.saving}
        onKeyStatusChange={() => void refreshSttAndKeys()}
      />
    </div>

    {#if readOnly}
      <p class="workshop-faint mt-3 text-xs leading-relaxed">
        Model picks are managed on your workshop host.
      </p>
    {/if}

    <details class="prefs-more mt-3" bind:open={modelsAdvancedOpen}>
      <summary class="prefs-more-summary">
        <span class="prefs-more-summary-copy">
          <span>Stages & providers</span>
          <span class="prefs-more-summary-meta">Routing and API keys</span>
        </span>
        <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
      </summary>
      <div class="prefs-more-body">
        <div class="agent-extra">
          <button
            type="button"
            class="agent-extra-btn"
            class:agent-extra-btn-active={modelsExtra === "stages"}
            aria-expanded={modelsExtra === "stages"}
            onclick={() => toggleModelsExtra("stages")}
          >
            Stages
          </button>
          <button
            type="button"
            class="agent-extra-btn"
            class:agent-extra-btn-active={modelsExtra === "providers"}
            aria-expanded={modelsExtra === "providers"}
            onclick={() => toggleModelsExtra("providers")}
          >
            Providers
          </button>
        </div>

        {#if modelsExtra === "stages"}
          <div class="agent-extra-panel mt-3">
            <p class="settings-subsection-lead mb-3">Stage routes need Save below.</p>
            <ModelsStagesTab disabled={readOnly || workshopDefaults.saving} {mobile} />
          </div>
        {:else if modelsExtra === "providers"}
          <div class="agent-extra-panel mt-3">
            <ProvidersSettingsTab
              {catalog}
              disabled={readOnly || workshopDefaults.saving}
              onKeysChanged={() => void refreshSttAndKeys()}
            />
          </div>
        {:else}
          <p class="prefs-footnote">Pick Stages or Providers to open that panel.</p>
        {/if}
      </div>
    </details>
  </div>

  <details class="prefs-more" bind:open={modeTransitionsOpen}>
    <summary class="prefs-more-summary">
      <span class="prefs-more-summary-copy">
        <span>Mode suggestions</span>
        <span class="prefs-more-summary-meta">
          {modePolicy.auto_accept === "never" ? "Ask first" : "Auto-accept enabled"}
        </span>
      </span>
      <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
    </summary>
    <div class="prefs-more-body">
      <p class="prefs-footnote mb-3">
        Medousa may suggest a better mode at a turn boundary. These controls decide how long the
        suggestion waits and whether it can apply automatically for the next turn.
      </p>
      <div class="prefs-grid">
        <label class="prefs-tile">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Auto-accept</span>
            <span class="prefs-tile-meta">How much transition authority Medousa has</span>
          </span>
          <select
            class="prefs-endpoint-input"
            value={modePolicy.auto_accept}
            disabled={readOnly || modePolicySaving}
            aria-label="Mode suggestion auto-accept policy"
            onchange={(event) =>
              void saveModePolicy({
                ...modePolicy,
                auto_accept: (event.currentTarget as HTMLSelectElement)
                  .value as AgentModeAutoAccept,
              })}
          >
            <option value="never">Never — ask me</option>
            <option value="task">Task-scoped only</option>
            <option value="all">All mode suggestions</option>
          </select>
        </label>

        <label class="prefs-tile prefs-tile-metric">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Suggestion expiry</span>
            <span class="prefs-tile-meta">5 seconds to 24 hours</span>
          </span>
          <span class="prefs-metric">
            <input
              type="number"
              class="prefs-metric-input prefs-metric-input-wide"
              min={5}
              max={86400}
              step={5}
              inputmode="numeric"
              value={modePolicy.proposal_ttl_seconds}
              readonly={readOnly}
              disabled={readOnly || modePolicySaving}
              aria-label="Mode suggestion expiry in seconds"
              onchange={(event) => {
                const value = Number((event.currentTarget as HTMLInputElement).value);
                if (Number.isFinite(value)) {
                  void saveModePolicy({
                    ...modePolicy,
                    proposal_ttl_seconds: Math.round(value),
                  });
                }
              }}
            />
            <span class="prefs-metric-unit">sec</span>
          </span>
        </label>
      </div>
      <p class="prefs-footnote mt-3">
        Coder still requires a chat bound to a Forge undertaking. Auto-accept changes mode state;
        it does not bypass Coder's worktree or tool fences.
      </p>
      {#if modePolicyFeedback}
        <p class="mt-2 text-xs text-surface-400" role="status">{modePolicyFeedback}</p>
      {/if}
    </div>
  </details>

  <details class="prefs-more" bind:open={memoryOpen}>
    <summary class="prefs-more-summary">
      <span class="prefs-more-summary-copy">
        <span>Memory</span>
        <span class="prefs-more-summary-meta">{memorySummary}</span>
      </span>
      <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
    </summary>
    <div class="prefs-more-body">
      <p class="prefs-footnote mb-3">
        Hot = recent turns · cold = older recall. Profiles still live in the sidebar.
      </p>
      <div class="prefs-grid">
        {#each memoryPrimary as field (field.key)}
          <label class="prefs-tile prefs-tile-metric">
            <span class="prefs-tile-copy">
              <span class="prefs-tile-title">{field.label}</span>
              <span class="prefs-tile-meta">{field.hint}</span>
            </span>
            <span class="prefs-metric">
              <input
                type="number"
                class="prefs-metric-input"
                min={field.min}
                max={field.max}
                step={1}
                inputmode="numeric"
                value={workshopDefaults.draft[field.key] ?? ""}
                readonly={readOnly}
                disabled={readOnly}
                aria-label="{field.label} in {field.unit}"
                oninput={(event) => numField(field.key, event)}
              />
              <span class="prefs-metric-unit">{field.unit}</span>
            </span>
          </label>
        {/each}
      </div>

      <details class="prefs-nested" bind:open={memoryBudgetsOpen}>
        <summary class="prefs-nested-summary">
          <span>Prompt budgets</span>
          <ChevronDown size={12} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
        </summary>
        <div class="prefs-grid mt-2">
          {#each memoryBudgets as field (field.key)}
            <label class="prefs-tile prefs-tile-metric">
              <span class="prefs-tile-copy">
                <span class="prefs-tile-title">{field.label}</span>
                <span class="prefs-tile-meta">{field.hint}</span>
              </span>
              <span class="prefs-metric">
                <input
                  type="number"
                  class="prefs-metric-input {field.wide ? 'prefs-metric-input-wide' : ''}"
                  min={field.min}
                  max={field.max}
                  step={field.step ?? 1}
                  inputmode="numeric"
                  value={workshopDefaults.draft[field.key] ?? ""}
                  readonly={readOnly}
                  disabled={readOnly}
                  aria-label="{field.label} in {field.unit}"
                  oninput={(event) => numField(field.key, event)}
                />
                <span class="prefs-metric-unit">{field.unit}</span>
              </span>
            </label>
          {/each}
        </div>
      </details>
    </div>
  </details>

  <details class="prefs-more" bind:open={voicesOpen}>
    <summary class="prefs-more-summary">
      <span class="prefs-more-summary-copy">
        <span>Custom voices</span>
        {#if customPresets.length > 0}
          <span class="prefs-more-summary-meta">{customPresets.length} saved</span>
        {/if}
      </span>
      <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
    </summary>
    <div class="prefs-more-body">
      {#if customPresets.length > 0 && !readOnly}
        <ul class="agent-voice-list">
          {#each customPresets as preset (preset.id)}
            <li class="prefs-tile agent-voice-row">
              <span class="prefs-tile-copy">
                <span class="prefs-tile-title">{preset.name}</span>
                {#if preset.description}
                  <span class="prefs-tile-meta">{preset.description}</span>
                {/if}
              </span>
              <span class="agent-voice-actions">
                <button
                  type="button"
                  class="btn btn-xs variant-soft"
                  disabled={workshopDefaults.saving}
                  onclick={() => openEditEditor(preset)}
                >
                  Edit
                </button>
                <button
                  type="button"
                  class="btn btn-xs variant-soft"
                  disabled={workshopDefaults.saving}
                  onclick={() => deleteCustomPreset(preset.id)}
                >
                  Delete
                </button>
              </span>
            </li>
          {/each}
        </ul>
      {/if}

      {#if !readOnly}
        {#if !editorOpen}
          <button
            type="button"
            class="btn btn-sm variant-soft mt-2"
            disabled={!canAddCustom || workshopDefaults.saving}
            onclick={openCreateEditor}
          >
            Add custom voice
          </button>
          {#if !canAddCustom}
            <p class="prefs-footnote">Up to {MAX_CUSTOM_VOICE_PRESETS} custom voices.</p>
          {/if}
        {:else}
          <div class="agent-voice-editor mt-2 space-y-3">
            <label class="block">
              <span class="workshop-label">Name</span>
              <input
                class="input mt-1 w-full"
                bind:value={draftName}
                maxlength={40}
                placeholder="Briefings"
              />
            </label>
            <label class="block">
              <span class="workshop-label">Description</span>
              <input
                class="input mt-1 w-full"
                bind:value={draftDescription}
                maxlength={120}
                placeholder="Optional one-liner"
              />
            </label>
            <label class="block">
              <span class="workshop-label">Stance</span>
              <textarea
                class="textarea mt-1 min-h-24 w-full resize-y"
                bind:value={draftAppendix}
                maxlength={600}
                placeholder="How she should answer in this voice…"
              ></textarea>
            </label>
            <div class="flex flex-wrap gap-2">
              <button
                type="button"
                class="btn btn-sm variant-filled-primary"
                disabled={workshopDefaults.saving || !draftName.trim() || !draftAppendix.trim()}
                onclick={saveEditor}
              >
                {editingId ? "Save changes" : "Create voice"}
              </button>
              <button
                type="button"
                class="btn btn-sm variant-soft"
                disabled={workshopDefaults.saving}
                onclick={resetEditor}
              >
                Cancel
              </button>
            </div>
          </div>
        {/if}
      {/if}
    </div>
  </details>

  <details class="prefs-more" bind:open={presentationsOpen}>
    <summary class="prefs-more-summary">
      <span>Presentations cleanup</span>
      <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
    </summary>
    <div class="prefs-more-body">
      <SettingsPresentationRetention {mobile} />
    </div>
  </details>

  <details class="prefs-more" bind:open={storageOpen}>
    <summary class="prefs-more-summary">
      <span>Workshop storage</span>
      <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
    </summary>
    <div class="prefs-more-body">
      <SettingsStorageGovernance {mobile} />
    </div>
  </details>

  {#if modelsExtra !== "stages"}
    <div class="agent-save mt-6 border-t border-surface-500/35 pt-5">
      <SettingsCharterSaveBar {mobile} />
    </div>
  {/if}
</section>

<style>
  .prefs {
    --prefs-gap: 0.5rem;
    --prefs-tile-radius: 0.65rem;
    --prefs-tile-pad: 0.55rem 0.75rem;
    --prefs-tile-min-h: 3.25rem;
    --prefs-tile-border: rgb(var(--color-surface-500) / 0.32);
    --prefs-tile-bg: rgb(var(--color-surface-900) / 0.28);
  }

  .prefs-band {
    margin-top: 1.25rem;
  }

  .prefs-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .prefs-band-head .settings-subsection-lead {
    margin-bottom: 0.6rem;
  }

  .prefs-stack {
    display: grid;
    gap: var(--prefs-gap);
  }

  .prefs-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: var(--prefs-gap);
  }

  @media (min-width: 720px) {
    .prefs-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .agent-active {
    display: grid;
    gap: var(--prefs-gap);
  }

  .agent-active-trigger {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    width: 100%;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .agent-active-trigger:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .agent-active-trigger:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .agent-active-trigger-open {
    border-color: rgb(var(--color-primary-500) / 0.35);
    background: rgb(var(--color-primary-500) / 0.08);
  }

  .agent-active-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.08rem;
  }

  .agent-active-kicker {
    font-size: 0.65rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .agent-active-title {
    font-size: 0.85rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .agent-active-meta {
    font-size: 0.7rem;
    line-height: 1.35;
    color: rgb(var(--color-surface-500));
  }

  .agent-active-action {
    flex-shrink: 0;
    font-size: 0.72rem;
    font-weight: 600;
  }

  .agent-picker {
    padding: 0.15rem 0 0.1rem;
  }

  .prefs-choice {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .prefs-choice:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .prefs-choice:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .prefs-choice-active {
    border-color: rgb(var(--color-primary-500) / 0.4);
    background: rgb(var(--color-primary-500) / 0.1);
    box-shadow: inset 0 0 0 1px rgb(var(--color-primary-500) / 0.18);
  }

  .prefs-choice-label {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-choice-hint {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-500));
  }

  .prefs-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
  }

  .prefs-tile-metric {
    cursor: default;
  }

  .prefs-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .prefs-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-tile-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-500));
  }

  .prefs-metric {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    flex-shrink: 0;
  }

  .prefs-metric-input {
    width: 3.4rem;
    border-radius: 0.4rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    background: rgb(var(--color-surface-950) / 0.45);
    padding: 0.2rem 0.35rem;
    text-align: right;
    font-size: 0.8rem;
    color: rgb(var(--color-surface-100));
  }

  .prefs-metric-input-wide {
    width: 4.5rem;
  }

  .prefs-metric-input:disabled {
    opacity: 0.5;
  }

  .prefs-metric-unit {
    font-size: 0.68rem;
    color: rgb(var(--color-surface-500));
  }

  .agent-extra {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .agent-extra-btn {
    border-radius: 999px;
    border: 1px solid var(--prefs-tile-border);
    background: transparent;
    padding: 0.28rem 0.7rem;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
  }

  .agent-extra-btn:hover {
    border-color: rgb(var(--color-surface-500) / 0.5);
    color: rgb(var(--color-surface-200));
  }

  .agent-extra-btn-active {
    border-color: rgb(var(--color-primary-500) / 0.4);
    background: rgb(var(--color-primary-500) / 0.1);
    color: rgb(var(--color-surface-100));
  }

  .agent-extra-panel {
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
    padding: 0.75rem;
  }

  .prefs-more {
    margin-top: 1rem;
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
  }

  .prefs-more-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
    list-style: none;
  }

  .prefs-more-summary::-webkit-details-marker {
    display: none;
  }

  .prefs-more-summary-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .prefs-more-summary-meta {
    font-size: 0.68rem;
    font-weight: 500;
    color: rgb(var(--color-surface-500));
  }

  :global(.prefs-more-chevron) {
    transition: transform 160ms ease;
  }

  .prefs-more[open] :global(.prefs-more-chevron),
  .prefs-nested[open] :global(.prefs-more-chevron) {
    transform: rotate(180deg);
  }

  .prefs-more-body {
    padding: 0 0.75rem 0.75rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
  }

  .prefs-nested {
    margin-top: 0.75rem;
  }

  .prefs-nested-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.35rem 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
    list-style: none;
  }

  .prefs-nested-summary::-webkit-details-marker {
    display: none;
  }

  .prefs-footnote {
    margin: 0.45rem 0 0;
    font-size: 0.7rem;
    color: rgb(var(--color-surface-500));
  }

  .agent-voice-list {
    display: grid;
    gap: var(--prefs-gap);
    margin: 0.55rem 0 0;
    padding: 0;
    list-style: none;
  }

  .agent-voice-row {
    cursor: default;
  }

  .agent-voice-actions {
    display: inline-flex;
    flex-shrink: 0;
    gap: 0.35rem;
  }

  .agent-models :global(.settings-profile-card) {
    border-radius: var(--prefs-tile-radius);
  }
</style>
