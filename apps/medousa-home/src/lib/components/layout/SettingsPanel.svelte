<script lang="ts">
  import SettingsNav from "$lib/components/settings/SettingsNav.svelte";
  import SettingsRoomSection from "$lib/components/settings/SettingsRoomSection.svelte";
  import SettingsRhythmSection from "$lib/components/settings/SettingsRhythmSection.svelte";
  import SettingsMemorySection from "$lib/components/settings/SettingsMemorySection.svelte";
  import SettingsModelsSection from "$lib/components/settings/SettingsModelsSection.svelte";
  import SettingsVoiceSection from "$lib/components/settings/SettingsVoiceSection.svelte";
  import SettingsReachSection from "$lib/components/settings/SettingsReachSection.svelte";
  import SettingsPhoneSection from "$lib/components/settings/SettingsPhoneSection.svelte";
  import SettingsBasementSection from "$lib/components/settings/SettingsBasementSection.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { depthModeLabel } from "$lib/utils/chatModelPicker";
  import { formatModelDisplayName } from "$lib/utils/formatModelDisplay";
  import type { SettingsSectionId } from "$lib/types/settings";

  interface Props {
    visible: boolean;
    revision: number;
    health: DaemonHealth | null;
    onDaemonHealth: () => void | Promise<void>;
    mobile?: boolean;
    embedded?: boolean;
  }

  let {
    visible,
    revision,
    health,
    onDaemonHealth,
    mobile = false,
    embedded = false,
  }: Props = $props();

  let activeSection = $state<SettingsSectionId>("room");

  $effect(() => {
    if (mobile) {
      activeSection = "room";
    }
  });

  $effect(() => {
    if (visible) {
      const pending = settingsNav.takePending();
      if (pending) activeSection = pending;
      void workshopDefaults.load();
    }
  });

  const charterLine = $derived(
    !workshopDefaults.loaded
      ? "Shape how Medousa listens, thinks, and remembers."
      : `${depthModeLabel(workshopDefaults.draft.responseDepthMode ?? "standard")} answers · ${formatModelDisplayName(workshopDefaults.draft.model ?? "model")} in chat`,
  );
</script>

<section class="settings-panel flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !embedded}
    <header class="workshop-header">
      <h1 class="text-base font-semibold text-surface-50">Settings</h1>
      <p class="workshop-header-line mt-1">
        {charterLine}
      </p>
    </header>
  {:else if mobile}
    <p class="settings-charter-line border-b border-surface-500/35 px-4 py-2 text-xs text-surface-400">
      {charterLine}
    </p>
  {/if}

  <div class="settings-shell min-h-0 flex-1 {mobile ? 'flex flex-col' : 'flex'}">
    <aside class="settings-shell-nav shrink-0 {mobile ? 'px-4 pt-3' : 'border-r border-surface-500/35 p-3'}">
      <SettingsNav
        active={activeSection}
        {mobile}
        onSelect={(section) => {
          activeSection = section;
        }}
      />
    </aside>

    <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto px-4 py-4">
      {#if activeSection === "room"}
        <SettingsRoomSection />
      {:else if activeSection === "rhythm"}
        <SettingsRhythmSection {mobile} />
      {:else if activeSection === "memory"}
        <SettingsMemorySection {mobile} />
      {:else if activeSection === "models"}
        <SettingsModelsSection {mobile} />
      {:else if activeSection === "voice"}
        <SettingsVoiceSection {mobile} />
      {:else if activeSection === "reach"}
        <SettingsReachSection {mobile} />
      {:else if activeSection === "phone"}
        <SettingsPhoneSection {mobile} />
      {:else}
        <SettingsBasementSection {revision} {health} {onDaemonHealth} {mobile} />
      {/if}
    </div>
  </div>
</section>
