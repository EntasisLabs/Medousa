<script lang="ts">
  import { ChevronDown } from "@lucide/svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { haptic } from "$lib/haptics";

  interface Props {
    /** Hide when only the default profile exists. */
    hideWhenSingle?: boolean;
  }

  let { hideWhenSingle = true }: Props = $props();

  let sheetOpen = $state(false);

  const visible = $derived(!hideWhenSingle || userProfiles.hasMultipleProfiles);

  async function pickProfile(profileId: string) {
    haptic("light");
    await userProfiles.setActive(profileId);
    sheetOpen = false;
  }

  function openSheet() {
    if (userProfiles.saving) return;
    haptic("light");
    sheetOpen = true;
    if (userProfiles.profiles.length === 0 && !userProfiles.loading) {
      void userProfiles.load();
    }
  }
</script>

{#if visible}
  <button
    type="button"
    class="mobile-profile-pill shrink-0"
    aria-label="Switch profile — {userProfiles.activeDisplayName}"
    aria-haspopup="dialog"
    aria-expanded={sheetOpen}
    disabled={userProfiles.saving}
    onclick={openSheet}
  >
    <span class="mobile-profile-monogram" aria-hidden="true">{userProfiles.profileMonogram}</span>
    <span class="max-w-[5.5rem] truncate text-xs font-medium text-surface-200">
      {userProfiles.activeDisplayName}
    </span>
    <ChevronDown size={14} class="shrink-0 text-surface-500" strokeWidth={2} />
  </button>
{/if}

{#if sheetOpen}
  <div
    class="mobile-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) sheetOpen = false;
    }}
  >
    <div class="mobile-sheet" role="dialog" aria-label="Switch profile">
      <header class="mobile-sheet-header">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold text-surface-50">You · profile</h2>
          <p class="workshop-faint mt-0.5 text-xs">Work and home stay separate</p>
        </div>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface shrink-0"
          onclick={() => {
            sheetOpen = false;
          }}
        >
          Done
        </button>
      </header>

      <div class="mobile-you-scroll px-4 pb-6 pt-2">
        {#if userProfiles.loading && userProfiles.profiles.length === 0}
          <p class="workshop-faint text-sm">Loading profiles…</p>
        {:else if userProfiles.error}
          <p class="text-sm text-error-400">{userProfiles.error}</p>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface mt-3"
            onclick={() => userProfiles.load()}
          >
            Retry
          </button>
        {:else}
          <div class="settings-profile-quick-row">
            {#each userProfiles.profiles as profile (profile.profile_id)}
              <button
                type="button"
                class="settings-profile-quick-btn {profile.profile_id === userProfiles.activeProfileId
                  ? 'settings-profile-quick-btn-active'
                  : ''}"
                disabled={userProfiles.saving}
                onclick={() => pickProfile(profile.profile_id)}
              >
                {profile.display_name}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
