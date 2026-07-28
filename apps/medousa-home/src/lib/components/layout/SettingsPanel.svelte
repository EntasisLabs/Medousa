<script lang="ts">
  import { onDestroy } from "svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import SettingsNav from "$lib/components/settings/SettingsNav.svelte";
  import SettingsPreferencesSection from "$lib/components/settings/SettingsPreferencesSection.svelte";
  import SettingsAgentSection from "$lib/components/settings/SettingsAgentSection.svelte";
  import SettingsRuntimeSection from "$lib/components/settings/SettingsRuntimeSection.svelte";
  import SettingsNetworkSection from "$lib/components/settings/SettingsNetworkSection.svelte";
  import SettingsConnectionsSection from "$lib/components/settings/SettingsConnectionsSection.svelte";
  import SettingsBasementSection from "$lib/components/settings/SettingsBasementSection.svelte";
  import SettingsPackagesSection from "$lib/components/settings/SettingsPackagesSection.svelte";
  import SettingsMcpSection from "$lib/components/settings/SettingsMcpSection.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { depthModeLabel } from "$lib/utils/chatModelPicker";
  import { formatModelDisplayName } from "$lib/utils/formatModelDisplay";
  import { peerUnreadCount } from "$lib/utils/lanShareApi";
  import { appUpdate } from "$lib/stores/appUpdate.svelte";
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

  let nearbyUnread = $state(0);
  let unreadTimer: ReturnType<typeof setInterval> | null = null;
  const activeSection = $derived(settingsNav.activeSection);
  const shellNav = $derived(!mobile && !embedded);

  async function refreshNearbyUnread() {
    if (!isTauri()) return;
    try {
      nearbyUnread = await peerUnreadCount();
    } catch {
      nearbyUnread = 0;
    }
  }

  $effect(() => {
    if (visible) {
      settingsNav.takePending();
      void workshopDefaults.load();
      void userProfiles.load({ suppressRemoteNotice: true });
      // Shared mode is probed from Sharing / chat — not on every Settings open.
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

  $effect(() => {
    if (!visible) return;
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      if (!workshopDefaults.dirty) return;
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  });

  onDestroy(() => {
    if (unreadTimer) clearInterval(unreadTimer);
  });

  const charterLine = $derived(
    !workshopDefaults.loaded
      ? "Shape how Medousa listens, thinks, and remembers."
      : `${depthModeLabel(workshopDefaults.draft.responseDepthMode ?? "standard")} answers · ${formatModelDisplayName(workshopDefaults.draft.model ?? "model")} in chat`,
  );

  const navBadges = $derived.by(() => {
    const badges: Partial<Record<SettingsSectionId, number>> = {};
    if (nearbyUnread > 0) badges.network = nearbyUnread;
    if (appUpdate.updateAvailable) badges.basement = 1;
    return badges;
  });
</script>

<section class="settings-panel flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !embedded}
    <header class="workshop-header">
      <div class="flex items-start gap-2">
        <ShellSidebarExpandButton label="Show settings sections" />
        <div class="min-w-0 flex-1">
          <h1 class="text-base font-semibold text-surface-50">Settings</h1>
          <p class="workshop-header-line mt-1">
            {charterLine}
          </p>
        </div>
      </div>
    </header>
  {/if}

  <div class="settings-shell min-h-0 flex-1 {mobile ? 'flex flex-col' : 'flex'}">
    {#if !shellNav}
      <aside class="settings-shell-nav mobile-you-scroll min-h-0 shrink-0 overflow-y-auto {mobile ? 'px-4 pt-3' : 'border-r border-surface-500/35 p-3'}">
        <SettingsNav
          active={activeSection}
          {mobile}
          badges={navBadges}
          onSelect={(section) => settingsNav.setActiveSection(section)}
        />
      </aside>
    {/if}

    <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto px-4 py-4">
      {#if activeSection === "preferences"}
        <SettingsPreferencesSection {mobile} />
      {:else if activeSection === "agent"}
        <SettingsAgentSection {mobile} />
      {:else if activeSection === "runtime"}
        <SettingsRuntimeSection {mobile} />
      {:else if activeSection === "network"}
        <SettingsNetworkSection {mobile} {visible} {health} />
      {:else if activeSection === "connections"}
        <SettingsConnectionsSection />
      {:else if activeSection === "packages"}
        <SettingsPackagesSection {mobile} />
      {:else if activeSection === "mcp"}
        <SettingsMcpSection {mobile} />
      {:else}
        <SettingsBasementSection {revision} {health} {onDaemonHealth} {mobile} />
      {/if}
    </div>
  </div>
</section>
