<script lang="ts">
  import {
    checkDaemonHealth,
    getDaemonUrl,
    setDaemonUrl,
    type DaemonHealth,
  } from "$lib/daemon";
  import { settings } from "$lib/stores/settings.svelte";

  interface Props {
    visible: boolean;
    revision: number;
    health: DaemonHealth | null;
    onDaemonHealth: () => void | Promise<void>;
  }

  let { visible, revision, health, onDaemonHealth }: Props = $props();

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
      const probe = await checkDaemonHealth();
      settings.daemonMessage = probe.ok ? "Connected" : probe.message;
      await onDaemonHealth();
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
    <p class="text-xs text-surface-400">Workshop connection and preferences</p>
  </header>

  <div class="flex-1 space-y-6 overflow-y-auto px-5 py-5">
    <section class="rounded-container-token border border-surface-500/20 bg-surface-900/50 p-4">
      <h2 class="text-sm font-semibold text-surface-100">Connection</h2>
      <p class="mt-1 text-xs text-surface-400">
        Where Medousa Home reaches the running workshop backend.
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
            class="text-xs {settings.daemonMessage === 'Connected' ||
            settings.daemonMessage.includes('connected')
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
      <div class="mt-4 flex items-center gap-4">
        <div
          class="flex h-12 w-28 shrink-0 overflow-hidden rounded-container-token border border-surface-500/30"
          aria-hidden="true"
        >
          <span class="flex-[2] bg-surface-950"></span>
          <span class="flex-1 bg-primary-500"></span>
          <span class="flex-[1.5] bg-surface-800"></span>
        </div>
        <div class="text-xs">
          <p class="font-medium text-surface-200">Obsidian</p>
          <p class="mt-0.5 text-surface-500">Near-black canvas, violet accent</p>
        </div>
      </div>
      <label class="mt-4 flex cursor-pointer items-center gap-3">
        <input
          type="checkbox"
          class="checkbox"
          checked={settings.darkMode}
          onchange={(event) =>
            settings.setDarkMode((event.currentTarget as HTMLInputElement).checked)}
        />
        <span class="text-sm text-surface-200">Dark mode (Obsidian theme)</span>
      </label>
    </section>

    <section class="rounded-container-token border border-surface-500/20 bg-surface-900/50 p-4">
      <h2 class="text-sm font-semibold text-surface-100">Activity feed</h2>
      <label class="mt-4 flex cursor-pointer items-center gap-3">
        <input
          type="checkbox"
          class="checkbox"
          checked={settings.showTechnicalActivity}
          onchange={(event) =>
            settings.setShowTechnicalActivity(
              (event.currentTarget as HTMLInputElement).checked,
            )}
        />
        <span class="text-sm text-surface-200">Show technical events</span>
      </label>
      <p class="mt-2 text-xs text-surface-500">
        Includes repeated job failures and internal workflow noise.
      </p>
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

    <section class="rounded-container-token border border-surface-500/20 bg-surface-900/50 p-4">
      <button
        type="button"
        class="flex w-full items-center justify-between text-left"
        onclick={() => (settings.diagnosticsOpen = !settings.diagnosticsOpen)}
      >
        <h2 class="text-sm font-semibold text-surface-100">Diagnostics</h2>
        <span class="text-xs text-surface-500">
          {settings.diagnosticsOpen ? "▾" : "▸"}
        </span>
      </button>
      {#if settings.diagnosticsOpen}
        <dl class="mt-4 space-y-2 text-xs">
          <div class="grid grid-cols-[7rem_1fr] gap-2">
            <dt class="text-surface-500">Status</dt>
            <dd class="font-mono text-surface-300">
              {health?.ok ? "connected" : "offline"}
            </dd>
          </div>
          <div class="grid grid-cols-[7rem_1fr] gap-2">
            <dt class="text-surface-500">Base URL</dt>
            <dd class="break-all font-mono text-surface-300">
              {settings.daemonUrl || "—"}
            </dd>
          </div>
          <div class="grid grid-cols-[7rem_1fr] gap-2">
            <dt class="text-surface-500">Backend</dt>
            <dd class="font-mono text-surface-300">{health?.backend ?? "—"}</dd>
          </div>
          <div class="grid grid-cols-[7rem_1fr] gap-2">
            <dt class="text-surface-500">Revision</dt>
            <dd class="font-mono text-surface-300">{revision}</dd>
          </div>
          <div class="grid grid-cols-[7rem_1fr] gap-2">
            <dt class="text-surface-500">Worker</dt>
            <dd class="font-mono text-surface-300">{health?.worker_id ?? "—"}</dd>
          </div>
          <div class="grid grid-cols-[7rem_1fr] gap-2">
            <dt class="text-surface-500">Tools</dt>
            <dd class="font-mono text-surface-300">
              {health?.tool_registry_count ?? "—"}
            </dd>
          </div>
          {#if health && !health.ok}
            <div class="grid grid-cols-[7rem_1fr] gap-2">
              <dt class="text-surface-500">Detail</dt>
              <dd class="break-all font-mono text-warning-400">{health.message}</dd>
            </div>
          {/if}
        </dl>
      {/if}
    </section>
  </div>
</section>
