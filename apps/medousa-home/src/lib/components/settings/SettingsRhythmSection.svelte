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
  </div>
</section>
