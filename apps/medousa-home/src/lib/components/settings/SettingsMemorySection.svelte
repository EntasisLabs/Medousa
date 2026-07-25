<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import SettingsPresentationRetention from "$lib/components/settings/SettingsPresentationRetention.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const fields = [
    {
      key: "sliceHotWindowTurns" as const,
      label: "Hot memory",
      hint: "How many recent turns stay in the active prompt window",
      unit: "turns",
      min: 2,
      max: 32,
      wide: false,
    },
    {
      key: "sliceColdWindowTurns" as const,
      label: "Cold recall",
      hint: "How far back older turns can still be pulled into a long thread",
      unit: "turns",
      min: 4,
      max: 64,
      wide: false,
    },
    {
      key: "activationDirectAnswerMaxPromptChars" as const,
      label: "Direct-answer budget",
      hint: "Max prompt size for a quick answer without a long dig",
      unit: "chars",
      min: 200,
      max: 20000,
      step: 20,
      wide: true,
    },
    {
      key: "activationLongSessionTurnThreshold" as const,
      label: "Long chat after",
      hint: "After this many turns, long-thread rules apply",
      unit: "turns",
      min: 8,
      max: 80,
      wide: false,
    },
    {
      key: "activationLongSessionMaxPromptChars" as const,
      label: "Long-chat budget",
      hint: "Max prompt size once a thread is considered long",
      unit: "chars",
      min: 200,
      max: 20000,
      step: 20,
      wide: true,
    },
  ];

  function numField(key: (typeof fields)[number]["key"], event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: Number.isFinite(value) ? value : null,
    };
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Memory</h2>
    <p class="workshop-faint mt-1 text-sm">
      How many turns stay in the prompt — hot = recent, cold = older recall — and when a long thread
      tightens context.
    </p>
    <p class="workshop-faint mt-2 text-xs">
      To teach who you are or switch work/home profiles, open
      <span class="text-surface-300">Profiles</span> in the sidebar.
    </p>
  </header>

  <div class="settings-toggle-list mt-5">
    {#each fields as field (field.key)}
      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">{field.label}</span>
          <span class="workshop-faint mt-0.5 block text-xs">{field.hint}</span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            class="settings-metric-input {field.wide ? 'settings-metric-input-wide' : ''}"
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
          <span class="settings-metric-unit" aria-hidden="true">{field.unit}</span>
        </span>
      </label>
    {/each}
  </div>

  <div class="mt-5">
    <SettingsCharterSaveBar {mobile} />
  </div>

  <SettingsPresentationRetention {mobile} />
</section>
