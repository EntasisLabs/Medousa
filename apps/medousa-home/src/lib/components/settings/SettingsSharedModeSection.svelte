<script lang="ts">
  import { onMount } from "svelte";
  import { sharedMode } from "$lib/stores/sharedMode.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { isTauri } from "$lib/window";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile || isTauriMobilePlatform());

  onMount(() => {
    void sharedMode.load();
    void userProfiles.load({ suppressRemoteNotice: true });
  });

  async function toggleShared(enabled: boolean) {
    if (readOnly || sharedMode.saving) return;
    await sharedMode.setMode(enabled ? "shared" : "personal");
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Shared</h2>
    <p class="workshop-faint mt-1 text-sm">
      Turn this brain into a team workshop — profiles become seats, vault stays shared.
    </p>
  </header>

  {#if !isTauri()}
    <p class="mt-5 text-sm text-surface-400">Shared mode is managed from the Medousa desktop app.</p>
  {:else}
    <div class="settings-toggle-list mt-5">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Shared mode</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            {#if sharedMode.isShared}
              Seats use pairing invites. Root administers settings; General is the room agent.
            {:else}
              Personal hats stay as today. Enable only on a workshop you want to share.
            {/if}
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={sharedMode.isShared}
          disabled={readOnly || sharedMode.loading || sharedMode.saving}
          onchange={(event) =>
            void toggleShared((event.currentTarget as HTMLInputElement).checked)}
          aria-label="Enable Shared mode"
        />
      </label>
    </div>

    {#if sharedMode.isShared}
      <div class="mt-4 rounded-xl border border-surface-500/35 bg-surface-950/40 px-4 py-3 text-sm text-surface-300">
        <p>
          Admin seat <span class="font-mono text-surface-100">{sharedMode.rootProfileId}</span>
          · room agent
          <span class="font-mono text-surface-100">{sharedMode.generalProfileId}</span>
        </p>
        <p class="workshop-faint mt-2 text-xs">
          Invite seats from Phone settings. New shared rooms appear in the chat rail.
        </p>
      </div>
    {/if}

    {#if sharedMode.error}
      <p class="mt-3 text-sm text-danger-300">{sharedMode.error}</p>
    {/if}
  {/if}
</section>
