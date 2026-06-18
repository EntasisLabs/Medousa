<script lang="ts">
  import { userProfiles } from "$lib/stores/userProfiles.svelte";

  interface Props {
    mobile?: boolean;
    compact?: boolean;
  }

  let { mobile = false, compact = false }: Props = $props();

  let createSlug = $state("");
  let createName = $state("");
  let showCreate = $state(false);

  $effect(() => {
    if (!userProfiles.loading && userProfiles.profiles.length === 0 && !userProfiles.error) {
      void userProfiles.load();
    }
  });

  async function switchProfile(profileId: string) {
    await userProfiles.setActive(profileId);
  }

  async function submitCreate(event: SubmitEvent) {
    event.preventDefault();
    const ok = await userProfiles.create(createSlug, createName);
    if (ok) {
      createSlug = "";
      createName = "";
      showCreate = false;
    }
  }
</script>

<article class="settings-profile-card {compact ? '' : 'mt-0'}">
  <header class="settings-profile-header">
    <div>
      <h3 class="settings-profile-title">You · profiles</h3>
      <p class="settings-profile-subtitle">
        Work and home stay separate — switch profile anytime. Each profile has its own memory and
        identity.
      </p>
    </div>
    <span
      class="settings-profile-status settings-profile-status-ok"
      title={userProfiles.resolvedUserId ?? undefined}
    >
      {userProfiles.activeDisplayName}
    </span>
  </header>

  {#if userProfiles.loading && userProfiles.profiles.length === 0}
    <p class="settings-profile-detail mt-3">Loading profiles…</p>
  {:else if userProfiles.error}
    <p class="mt-3 text-xs text-error-400">{userProfiles.error}</p>
    <button
      type="button"
      class="btn btn-sm variant-ghost-surface mt-2"
      onclick={() => userProfiles.load()}
    >
      Retry
    </button>
  {:else}
    <div class="settings-profile-quick mt-4">
      <span class="settings-profile-quick-label">Active profile</span>
      <div class="settings-profile-quick-row">
        {#each userProfiles.profiles as profile (profile.profile_id)}
          <button
            type="button"
            class="settings-profile-quick-btn {profile.profile_id === userProfiles.activeProfileId
              ? 'settings-profile-quick-btn-active'
              : ''}"
            disabled={userProfiles.saving}
            onclick={() => switchProfile(profile.profile_id)}
          >
            {profile.display_name}
          </button>
        {/each}
      </div>
    </div>

    {#if showCreate}
      <form class="mt-4 space-y-3" onsubmit={submitCreate}>
          <label class="block">
            <span class="workshop-label">Slug</span>
            <input
              class="input mt-1 w-full max-w-xs text-sm"
              type="text"
              placeholder="work"
              bind:value={createSlug}
              maxlength="32"
              autocomplete="off"
            />
            <span class="workshop-faint mt-1 block text-xs">Lowercase id — stored as user:slug</span>
          </label>
          <label class="block">
            <span class="workshop-label">Display name</span>
            <input
              class="input mt-1 w-full max-w-xs text-sm"
              type="text"
              placeholder="Work"
              bind:value={createName}
              maxlength="64"
            />
          </label>
          <div class="flex flex-wrap gap-2">
            <button
              type="submit"
              class="btn btn-sm variant-filled-primary"
              disabled={userProfiles.saving}
            >
              Create profile
            </button>
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              disabled={userProfiles.saving}
              onclick={() => {
                showCreate = false;
                userProfiles.clearMessage();
              }}
            >
              Cancel
            </button>
          </div>
        </form>
      {:else}
        <button
          type="button"
          class="workshop-text-action mt-4 text-sm"
          disabled={userProfiles.saving}
          onclick={() => {
            showCreate = true;
            userProfiles.clearMessage();
          }}
        >
          + Add profile (work, home, …)
        </button>
    {/if}

    {#if userProfiles.message}
      <p class="settings-profile-footnote mt-3 text-success-400">{userProfiles.message}</p>
    {/if}
  {/if}
</article>
