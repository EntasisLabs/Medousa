<script lang="ts">
  import { onMount } from "svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import {
    queryLiveActivityAvailability,
    type LiveActivityStatus,
  } from "$lib/liveActivity";
  import {
    workshopRetentionLocalHint,
    workshopRetentionReadHint,
  } from "$lib/platformCopy";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let liveActivityStatus = $state<LiveActivityStatus | null>(null);

  const retentionReadOnly = $derived(mobile || isTauriMobilePlatform());

  async function refreshLiveActivityStatus() {
    if (!mobile && !isTauriMobilePlatform()) return;
    liveActivityStatus = await queryLiveActivityAvailability();
  }

  onMount(() => {
    void refreshLiveActivityStatus();
  });
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Rhythm</h2>
    <p class="workshop-faint mt-1 text-sm">
      Notifications and display — most rhythm toggles are saved on this device.
    </p>
  </header>

  <div class="settings-toggle-list mt-5">
    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Work done notifications</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Notify when a work card reaches done
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={settings.notificationsEnabled}
        onchange={(event) =>
          settings.setNotificationsEnabled((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>

    {#if mobile}
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Remote push</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Notify this phone from your Mac when work finishes or needs attention — even when the app is closed
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={settings.remotePushEnabled}
          onchange={(event) =>
            settings.setRemotePushEnabled((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Live Activity</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Show Medousa working status on the Lock Screen and Dynamic Island while jobs run
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={settings.liveActivityEnabled}
          onchange={async (event) => {
            settings.setLiveActivityEnabled((event.currentTarget as HTMLInputElement).checked);
            await refreshLiveActivityStatus();
          }}
        />
      </label>
      {#if liveActivityStatus}
        <p class="workshop-faint -mt-2 mb-1 px-1 text-xs">
          {#if liveActivityStatus.error}
            Live Activity: {liveActivityStatus.error}
          {:else if liveActivityStatus.active}
            Live Activity: active on Lock Screen / Dynamic Island
          {:else if liveActivityStatus.available}
            Live Activity: ready — starts when work is in motion; Mac can keep it updated while backgrounded
          {:else}
            Live Activity: checking…
          {/if}
          {#if liveActivityStatus.diagnostics && !liveActivityStatus.available}
            <span class="mt-1 block font-mono text-[10px] text-surface-500">
              bridge={liveActivityStatus.diagnostics.bridgeLinked ? "yes" : "no"}
              · widget={liveActivityStatus.diagnostics.widgetExtensionInstalled ? "yes" : "no"}
              · plist={liveActivityStatus.diagnostics.supportsLiveActivities ? "yes" : "no"}
              · ios={liveActivityStatus.diagnostics.activitiesEnabled ? "on" : "off"}
            </span>
          {/if}
        </p>
      {/if}
      <p class="workshop-faint -mt-2 mb-1 px-1 text-xs">
        Home screen widget: add <strong class="font-medium text-surface-300">Pulse</strong> from the iOS widget gallery — updates while the app is open or when you return to it.
      </p>
    {/if}

    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Workshop guidance</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Journey steps, starter recipes, and friendly summaries in Workshop and Automations
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={settings.showWorkshopGuidance}
        onchange={(event) =>
          settings.setShowWorkshopGuidance((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>

    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Technical activity</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Show repeated job failures, turn lifecycle noise, and internal workflow events
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={settings.showTechnicalActivity}
        onchange={(event) =>
          settings.setShowTechnicalActivity((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>

    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Open Web when agent browses</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Switch to the Web surface when Medousa navigates or needs verification — turn off to stay on Chat
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={settings.autoOpenWebOnAgentBrowse}
        onchange={(event) =>
          settings.setAutoOpenWebOnAgentBrowse((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>

    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Engine details in chat</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Show orchestrator routing and tool telemetry in chat (hidden by default; never deleted)
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={settings.showEngineDetailsInChat}
        onchange={(event) =>
          settings.setShowEngineDetailsInChat((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>
  </div>

  <div class="mt-8 border-t border-surface-800/80 pt-6">
    <h3 class="text-sm font-semibold text-surface-100">Work card retention</h3>
    <p class="workshop-faint mt-1 text-xs">
      {#if retentionReadOnly}
        {workshopRetentionReadHint()}
      {:else}
        Saved to
        <span class="font-mono text-surface-400">tui_defaults.json</span>
        {workshopRetentionLocalHint()} Hide removes terminal cards from the board; wipe purges
        archived records.
      {/if}
    </p>
    <div class="mt-4 grid gap-4 sm:grid-cols-2">
      <label class="block">
        <span class="text-xs font-medium text-surface-200">Hide from board after (hours)</span>
        <input
          type="number"
          min="1"
          max="168"
          class="input mt-1 w-full"
          value={settings.workCardHideAfterHours}
          disabled={retentionReadOnly}
          onchange={(event) => {
            settings.setWorkCardHideAfterHours(
              Number((event.currentTarget as HTMLInputElement).value),
            );
            void settings.persistWorkRetention();
          }}
        />
      </label>
      <label class="block">
        <span class="text-xs font-medium text-surface-200">Wipe archived after (days)</span>
        <input
          type="number"
          min="1"
          max="90"
          class="input mt-1 w-full"
          value={settings.workCardWipeAfterDays}
          disabled={retentionReadOnly}
          onchange={(event) => {
            settings.setWorkCardWipeAfterDays(
              Number((event.currentTarget as HTMLInputElement).value),
            );
            void settings.persistWorkRetention();
          }}
        />
      </label>
    </div>
  </div>
</section>
