<script lang="ts">
  import { onMount } from "svelte";
  import MessagingChannelsSettings from "$lib/components/messaging/MessagingChannelsSettings.svelte";
  import PhonePairPanel from "$lib/components/pairing/PhonePairPanel.svelte";
  import SettingsLanShareSection from "$lib/components/settings/SettingsLanShareSection.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { workshopPairingManagedHint } from "$lib/platformCopy";
  import { sharedMode } from "$lib/stores/sharedMode.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { isTauri } from "$lib/window";

  interface Props {
    mobile?: boolean;
    visible?: boolean;
    health: DaemonHealth | null;
  }

  let { mobile = false, visible = true, health }: Props = $props();

  const readOnly = $derived(mobile || isTauriMobilePlatform());

  const networkSummary = $derived.by(() => {
    const mode = sharedMode.isShared ? "Shared" : "Personal";
    return `${mode} · phone, peers & channels`;
  });

  onMount(() => {
    void sharedMode.load();
    void userProfiles.load({ suppressRemoteNotice: true });
  });

  async function toggleShared(enabled: boolean) {
    if (readOnly || sharedMode.saving) return;
    await sharedMode.setMode(enabled ? "shared" : "personal");
  }
</script>

<section class="settings-section prefs network">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Sharing</h2>
    <p class="workshop-faint mt-1 text-sm">{networkSummary}</p>
  </header>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Shared</h3>
      <p class="settings-subsection-lead">
        Team seats on this brain — vault stays shared.
      </p>
    </div>

    {#if !isTauri()}
      <p class="text-sm text-surface-400">Shared mode is managed from the Medousa desktop app.</p>
    {:else}
      <div class="prefs-stack">
        <label class="prefs-tile">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Shared mode</span>
            <span class="prefs-tile-meta">
              {#if sharedMode.unsupported}
                Not available on this workshop yet
              {:else if sharedMode.isShared}
                Seats via Phone invites · admin {sharedMode.rootProfileId}
              {:else}
                Personal hats stay as today
              {/if}
            </span>
          </span>
          <input
            type="checkbox"
            class="prefs-switch"
            checked={sharedMode.isShared}
            disabled={readOnly ||
              sharedMode.loading ||
              sharedMode.saving ||
              sharedMode.unsupported}
            onchange={(event) =>
              void toggleShared((event.currentTarget as HTMLInputElement).checked)}
            aria-label="Enable Shared mode"
          />
        </label>
      </div>

      {#if sharedMode.error}
        <p class="mt-2 text-xs text-warning-300/90">{sharedMode.error}</p>
      {/if}
    {/if}
  </div>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Phone</h3>
      <p class="settings-subsection-lead">
        Pair as a second portal on the same Wi‑Fi.
      </p>
    </div>

    {#if mobile && isTauriMobilePlatform()}
      <div class="network-callout text-sm leading-relaxed text-surface-300">
        {workshopPairingManagedHint()} Then connect this app under Settings → Workshop using your
        workshop's LAN address.
      </div>
    {:else}
      <PhonePairPanel mode="settings" />
    {/if}
  </div>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Nearby</h3>
      <p class="settings-subsection-lead">
        Wi‑Fi reachability, peers, and canvas backups.
      </p>
    </div>

    <SettingsLanShareSection {mobile} embedded />
  </div>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Channels</h3>
      <p class="settings-subsection-lead">
        Messaging doors — open one to configure.
      </p>
    </div>

    <MessagingChannelsSettings {visible} {health} />
  </div>
</section>

<style>
  .prefs {
    --prefs-gap: 0.5rem;
    --prefs-tile-radius: 0.65rem;
    --prefs-tile-pad: 0.55rem 0.75rem;
    --prefs-tile-min-h: 3.25rem;
    --prefs-tile-border: rgb(var(--color-surface-500) / 0.32);
    --prefs-tile-bg: rgb(var(--color-surface-900) / 0.28);
  }

  .prefs-band {
    margin-top: 1.25rem;
  }

  .prefs-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .prefs-band-head .settings-subsection-lead {
    margin-bottom: 0.6rem;
  }

  .prefs-stack {
    display: grid;
    gap: var(--prefs-gap);
  }

  .prefs-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
  }

  .prefs-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .prefs-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-tile-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-500));
  }

  .prefs-switch {
    position: relative;
    flex-shrink: 0;
    width: 2.35rem;
    height: 1.3rem;
    margin: 0;
    appearance: none;
    border: 0;
    border-radius: 999px;
    background: rgb(var(--color-surface-600) / 0.55);
    cursor: pointer;
    transition: background 140ms ease;
  }

  .prefs-switch::after {
    content: "";
    position: absolute;
    top: 0.15rem;
    left: 0.15rem;
    width: 1rem;
    height: 1rem;
    border-radius: 999px;
    background: rgb(var(--color-surface-100));
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    transition: transform 140ms ease;
  }

  .prefs-switch:checked {
    background: rgb(var(--color-primary-500) / 0.85);
  }

  .prefs-switch:checked::after {
    transform: translateX(1.05rem);
  }

  .prefs-switch:focus-visible {
    outline: 2px solid rgb(var(--color-primary-400) / 0.7);
    outline-offset: 2px;
  }

  .prefs-switch:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
