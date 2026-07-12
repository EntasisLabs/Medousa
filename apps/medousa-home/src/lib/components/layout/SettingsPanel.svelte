<script lang="ts">
  import { onDestroy } from "svelte";
  import SettingsNav from "$lib/components/settings/SettingsNav.svelte";
  import SettingsRoomSection from "$lib/components/settings/SettingsRoomSection.svelte";
  import SettingsRhythmSection from "$lib/components/settings/SettingsRhythmSection.svelte";
  import SettingsMemorySection from "$lib/components/settings/SettingsMemorySection.svelte";
  import SettingsModelsSection from "$lib/components/settings/SettingsModelsSection.svelte";
  import SettingsVoiceSection from "$lib/components/settings/SettingsVoiceSection.svelte";
  import SettingsReachSection from "$lib/components/settings/SettingsReachSection.svelte";
  import SettingsEngineSection from "$lib/components/settings/SettingsEngineSection.svelte";
  import SettingsPhoneSection from "$lib/components/settings/SettingsPhoneSection.svelte";
  import SettingsLanShareSection from "$lib/components/settings/SettingsLanShareSection.svelte";
  import SettingsBasementSection from "$lib/components/settings/SettingsBasementSection.svelte";
  import SettingsCanvasSection from "$lib/components/settings/SettingsCanvasSection.svelte";
  import SettingsPackagesSection from "$lib/components/settings/SettingsPackagesSection.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { depthModeLabel } from "$lib/utils/chatModelPicker";
  import { formatModelDisplayName } from "$lib/utils/formatModelDisplay";
  import { peerUnreadCount } from "$lib/utils/lanShareApi";
  import { isTauri } from "$lib/window";
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
  let nearbyUnread = $state(0);
  let unreadTimer: ReturnType<typeof setInterval> | null = null;

  async function refreshNearbyUnread() {
    if (!isTauri()) return;
    try {
      nearbyUnread = await peerUnreadCount();
    } catch {
      nearbyUnread = 0;
    }
  }

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
      void userProfiles.load();
      void refreshNearbyUnread();
      if (!unreadTimer) {
        unreadTimer = setInterval(() => {
          void refreshNearbyUnread();
        }, 8000);
      }
    } else if (unreadTimer) {
      clearInterval(unreadTimer);
      unreadTimer = null;
    }
  });

  onDestroy(() => {
    if (unreadTimer) clearInterval(unreadTimer);
  });

  const charterLine = $derived(
    !workshopDefaults.loaded
      ? "Shape how Medousa listens, thinks, and remembers."
      : `${depthModeLabel(workshopDefaults.draft.responseDepthMode ?? "standard")} answers · ${formatModelDisplayName(workshopDefaults.draft.model ?? "model")} in chat`,
  );

  const navBadges = $derived(
    nearbyUnread > 0 ? ({ nearby: nearbyUnread } as Partial<Record<SettingsSectionId, number>>) : {},
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
        badges={navBadges}
        onSelect={(section) => {
          activeSection = section;
        }}
      />
    </aside>

    <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto px-4 py-4">
      {#if activeSection === "room"}
        <SettingsRoomSection />
      {:else if activeSection === "canvas"}
        <SettingsCanvasSection />
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
      {:else if activeSection === "engine"}
        <SettingsEngineSection {mobile} />
      {:else if activeSection === "phone"}
        <SettingsPhoneSection {mobile} />
      {:else if activeSection === "nearby"}
        <SettingsLanShareSection {mobile} />
      {:else if activeSection === "packages"}
        <SettingsPackagesSection {mobile} />
      {:else}
        <SettingsBasementSection {revision} {health} {onDaemonHealth} {mobile} />
      {/if}
    </div>
  </div>
</section>
