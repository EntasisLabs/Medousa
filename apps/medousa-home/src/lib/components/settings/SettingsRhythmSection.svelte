<script lang="ts">
  import { onMount } from "svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import {
    queryLiveActivityAvailability,
    type LiveActivityStatus,
  } from "$lib/liveActivity";
  import { hostComputerPhrase,
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

  function commitHideHours(raw: string) {
    settings.setWorkCardHideAfterHours(Number(raw));
    void settings.persistWorkRetention();
  }

  function commitWipeDays(raw: string) {
    settings.setWorkCardWipeAfterDays(Number(raw));
    void settings.persistWorkRetention();
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Rhythm</h2>
    <p class="workshop-faint mt-1 text-sm">
      Notifications and display — most rhythm toggles are saved on this device.
    </p>
  </header>

  <div class="mt-5">
    <h3 class="settings-subsection-heading">Work cards</h3>
    <p class="settings-subsection-lead">
      {#if retentionReadOnly}
        {workshopRetentionReadHint()}
      {:else}
        Finished cards leave the board first, then archives clear for good.
        <span class="block mt-0.5 opacity-80">{workshopRetentionLocalHint()}</span>
      {/if}
    </p>

    <div class="settings-toggle-list">
      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Hide from board</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            After a card is done, how long it stays visible
          </span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            min="1"
            max="168"
            inputmode="numeric"
            class="settings-metric-input"
            value={settings.workCardHideAfterHours}
            disabled={retentionReadOnly}
            aria-label="Hide from board after hours"
            onchange={(event) => commitHideHours((event.currentTarget as HTMLInputElement).value)}
          />
          <span class="settings-metric-unit" aria-hidden="true">hours</span>
        </span>
      </label>

      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Clear archives</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            How long hidden cards are kept before permanent wipe
          </span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            min="1"
            max="90"
            inputmode="numeric"
            class="settings-metric-input"
            value={settings.workCardWipeAfterDays}
            disabled={retentionReadOnly}
            aria-label="Clear archives after days"
            onchange={(event) => commitWipeDays((event.currentTarget as HTMLInputElement).value)}
          />
          <span class="settings-metric-unit" aria-hidden="true">days</span>
        </span>
      </label>
    </div>
  </div>

  <div class="settings-toggle-list mt-6">
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
            Notify this phone from {hostComputerPhrase()} when work finishes or needs attention — even when the app is closed
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

    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">
          Liquid chat <span class="workshop-faint">· experimental</span>
        </span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Render chat turns through the Liquid UI scene renderer — same content, composed as addressable components
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={settings.liquidChat}
        onchange={(event) =>
          settings.setLiquidChat((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>

    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Model picker in chat</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Show the model / depth / reasoning control in the composer (off by default — change models in Settings anytime)
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={settings.showChatModelPicker}
        onchange={(event) =>
          settings.setShowChatModelPicker((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>

    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Attachment tip in chat</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Show the “Up to N files…” hint above the composer (off by default)
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={settings.showChatAttachmentHint}
        onchange={(event) =>
          settings.setShowChatAttachmentHint((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>
  </div>
</section>
