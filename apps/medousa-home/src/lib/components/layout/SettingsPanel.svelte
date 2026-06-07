<script lang="ts">
  import {
    checkDaemonHealth,
    getDaemonUrl,
    setDaemonUrl,
  } from "$lib/daemon";
  import { settings } from "$lib/stores/settings.svelte";

  interface Props {
    visible: boolean;
    onDaemonHealth: (message: string) => void;
  }

  let { visible, onDaemonHealth }: Props = $props();

  $effect(() => {
    if (visible && !settings.daemonUrl) {
      void loadDaemonUrl();
    }
  });

  async function loadDaemonUrl() {
    try {
      settings.daemonUrl = await getDaemonUrl();
    } catch (err) {
      settings.daemonMessage =
        err instanceof Error ? err.message : String(err);
    }
  }

  async function saveDaemonUrl() {
    settings.savingDaemon = true;
    settings.daemonMessage = null;
    try {
      await setDaemonUrl(settings.daemonUrl);
      const health = await checkDaemonHealth();
      settings.daemonMessage = health.message;
      onDaemonHealth(health.message);
    } catch (err) {
      settings.daemonMessage =
        err instanceof Error ? err.message : String(err);
    } finally {
      settings.savingDaemon = false;
    }
  }
</script>

<section class="flex h-full min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="border-b border-surface-500/20 px-5 py-4">
    <h1 class="text-base font-semibold">Settings</h1>
    <p class="text-xs text-surface-400">Daemon connection and workshop preferences</p>
  </header>

  <div class="flex-1 space-y-6 overflow-y-auto px-5 py-5">
    <section class="rounded-container-token border border-surface-500/20 bg-surface-900/50 p-4">
      <h2 class="text-sm font-semibold text-surface-100">Daemon</h2>
      <p class="mt-1 text-xs text-surface-400">
        Medousa Home talks only to <code class="text-surface-300">medousa_daemon</code>.
      </p>
      <label class="mt-4 block text-xs text-surface-400" for="daemon-url">
        Base URL
      </label>
      <input
        id="daemon-url"
        class="input mt-1 w-full max-w-xl"
        bind:value={settings.daemonUrl}
        placeholder="http://127.0.0.1:7419"
      />
      <div class="mt-3 flex items-center gap-2">
        <button
          type="button"
          class="btn variant-filled-primary"
          disabled={settings.savingDaemon || !settings.daemonUrl.trim()}
          onclick={saveDaemonUrl}
        >
          {settings.savingDaemon ? "Saving…" : "Save & test"}
        </button>
        {#if settings.daemonMessage}
          <p
            class="text-xs {settings.daemonMessage.includes('connected')
              ? 'text-success-400'
              : 'text-warning-400'}"
          >
            {settings.daemonMessage}
          </p>
        {/if}
      </div>
    </section>

    <section class="rounded-container-token border border-surface-500/20 bg-surface-900/50 p-4">
      <h2 class="text-sm font-semibold text-surface-100">Appearance</h2>
      <label class="mt-4 flex cursor-pointer items-center gap-3">
        <input
          type="checkbox"
          class="checkbox"
          checked={settings.darkMode}
          onchange={(event) =>
            settings.setDarkMode((event.currentTarget as HTMLInputElement).checked)}
        />
        <span class="text-sm text-surface-200">Dark mode (sahara theme)</span>
      </label>
    </section>

    <section class="rounded-container-token border border-surface-500/20 bg-surface-900/50 p-4">
      <h2 class="text-sm font-semibold text-surface-100">Notifications</h2>
      <label class="mt-4 flex cursor-pointer items-center gap-3">
        <input
          type="checkbox"
          class="checkbox"
          checked={settings.notificationsEnabled}
          onchange={(event) =>
            settings.setNotificationsEnabled(
              (event.currentTarget as HTMLInputElement).checked,
            )}
        />
        <span class="text-sm text-surface-200">
          Notify when work cards reach done
        </span>
      </label>
    </section>
  </div>
</section>
