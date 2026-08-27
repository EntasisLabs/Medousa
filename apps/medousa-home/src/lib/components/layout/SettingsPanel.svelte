<script lang="ts">
  import "$lib/styles/settings.postcss";
  import { onDestroy } from "svelte";
  import LazyFeatureView from "$lib/components/layout/LazyFeatureView.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import SettingsNav from "$lib/components/settings/SettingsNav.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { depthModeLabel } from "$lib/utils/chatModelPicker";
  import { formatModelDisplayName } from "$lib/utils/formatModelDisplay";
  import { peerUnreadCount } from "$lib/utils/lanShareApi";
  import { appUpdate } from "$lib/stores/appUpdate.svelte";
  import { isTauri } from "$lib/window";
  import { isTauriDesktop } from "$lib/platform";
  import type { SettingsSectionId } from "$lib/types/settings";
  import {
    loadSettingsAgentSection,
    loadSettingsBasementSection,
    loadSettingsConnectionsSection,
    loadSettingsMcpSection,
    loadSettingsNetworkSection,
    loadSettingsPackagesSection,
    loadSettingsPreferencesSection,
    loadSettingsRuntimeSection,
  } from "$lib/runtime/viewLoaders";

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
  const nativeWorkloads = $derived(
    health?.runtime?.advertised_capabilities.includes("deployment.native-workloads") ??
      isTauriDesktop(),
  );
  const chatGptAccountAuth = $derived(
    health?.runtime?.advertised_capabilities.includes("auth.chatgpt-account") ??
      isTauriDesktop(),
  );
  const embeddedMcp = $derived(
    health?.runtime?.advertised_capabilities.includes("mcp.remote-config") ?? false,
  );
  const canManageMcp = $derived(
    embeddedMcp || (isTauriDesktop() && workshops.activeWorkshop?.kind === "local"),
  );

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
    {#if !shellNav && !mobile}
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
        <LazyFeatureView loader={loadSettingsPreferencesSection} />
      {:else if activeSection === "agent"}
        <LazyFeatureView loader={loadSettingsAgentSection} {nativeWorkloads} />
      {:else if activeSection === "runtime"}
        <LazyFeatureView loader={loadSettingsRuntimeSection} {nativeWorkloads} />
      {:else if activeSection === "network"}
        <LazyFeatureView
          loader={loadSettingsNetworkSection}
          {mobile}
          {visible}
          {health}
          {nativeWorkloads}
        />
      {:else if activeSection === "connections"}
        <LazyFeatureView loader={loadSettingsConnectionsSection} {chatGptAccountAuth} />
      {:else if activeSection === "packages"}
        <LazyFeatureView loader={loadSettingsPackagesSection} />
      {:else if activeSection === "mcp"}
        <LazyFeatureView loader={loadSettingsMcpSection} {embeddedMcp} {canManageMcp} {mobile} />
      {:else}
        <LazyFeatureView
          loader={loadSettingsBasementSection}
          {revision}
          {health}
          {onDaemonHealth}
          {mobile}
        />
      {/if}
    </div>
  </div>
</section>
