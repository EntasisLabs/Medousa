<script lang="ts">
  import SettingsNav from "$lib/components/settings/SettingsNav.svelte";
  import SettingsRoomSection from "$lib/components/settings/SettingsRoomSection.svelte";
  import SettingsRhythmSection from "$lib/components/settings/SettingsRhythmSection.svelte";
  import SettingsMemorySection from "$lib/components/settings/SettingsMemorySection.svelte";
  import SettingsVoiceSection from "$lib/components/settings/SettingsVoiceSection.svelte";
  import SettingsReachSection from "$lib/components/settings/SettingsReachSection.svelte";
  import SettingsBasementSection from "$lib/components/settings/SettingsBasementSection.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
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
      void workshopDefaults.load();
    }
  });

  const charterLine = $derived(
    !workshopDefaults.loaded
      ? "Your charter with the workshop."
      : (() => {
          const hot = workshopDefaults.draft.sliceHotWindowTurns ?? 8;
          const depth = workshopDefaults.draft.responseDepthMode ?? "standard";
          const model = workshopDefaults.draft.model?.trim() || "default model";
          const modules = workshopDefaults.allowedModulesText
            .split(/[,\n]/)
            .map((entry) => entry.trim())
            .filter(Boolean);
          const reach =
            modules.length > 0 ? `${modules.length} tools allowed` : "full tool catalog";
          return `Remembers ${hot} turns hot · answers ${depth} · ${reach} · ${model}`;
        })(),
  );
</script>

<section class="settings-panel flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !embedded}
    <header class="workshop-header">
      <h1 class="text-base font-semibold text-surface-50">Settings</h1>
      <p class="settings-charter-line mt-1 text-xs text-surface-300">
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
      {:else if activeSection === "voice"}
        <SettingsVoiceSection {mobile} />
      {:else if activeSection === "reach"}
        <SettingsReachSection {mobile} />
      {:else}
        <SettingsBasementSection {revision} {health} {onDaemonHealth} {mobile} />
      {/if}
    </div>
  </div>
</section>
