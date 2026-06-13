<script lang="ts">
  import { settings } from "$lib/stores/settings.svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Rhythm</h2>
    <p class="workshop-faint mt-1 text-sm">
      How Medousa interrupts your day on this device{mobile ? " — saved locally" : ""}.
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

    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Technical activity</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Show repeated job failures and internal workflow noise in the feed
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
        <span class="block text-sm font-medium text-surface-100">Engine details in chat</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Show orchestrator routing and tool telemetry in the chat stream (hidden by default)
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
      Saved to <span class="font-mono text-surface-400">tui_defaults.json</span> on the Mac daemon.
      Hide removes terminal cards from the board; wipe purges archived records.
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
