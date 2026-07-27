<script lang="ts">
  import { Brain } from "@lucide/svelte";
  import { identity } from "$lib/stores/identity.svelte";
  import { profilesSelection } from "$lib/stores/profilesSelection.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { dispatchProfilesFocusTeach } from "$lib/utils/profilesChromeEvents";
  import { buildProfileShelfEntries, profileKindLabel } from "$lib/utils/profileShelf";
  import { onMount } from "svelte";

  interface Props {
    onPickProfile?: (profileId: string) => void;
    chrome?: "default" | "rail-list";
  }

  let { onPickProfile, chrome = "rail-list" }: Props = $props();

  const entries = $derived(
    identity.context ? buildProfileShelfEntries(identity.context) : [],
  );

  onMount(() => {
    if (userProfiles.profiles.length === 0) {
      void userProfiles.load();
    }
    void identity.refresh({ relationshipLimit: 32 });
  });

  async function switchProfile(profileId: string) {
    await userProfiles.setActive(profileId);
    profilesSelection.select(null);
    await identity.refresh({ relationshipLimit: 32 });
    onPickProfile?.(profileId);
  }

  function pickEntry(entryId: string) {
    profilesSelection.select(entryId);
    onPickProfile?.(userProfiles.activeProfileId ?? "");
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-chrome={chrome}>
  {#if userProfiles.hasMultipleProfiles}
    <div class="shrink-0 border-b border-surface-500/30 px-1.5 py-1.5">
      <div class="flex flex-wrap gap-1">
        {#each userProfiles.profiles as profile (profile.profile_id)}
          <button
            type="button"
            class="rounded-md px-2 py-1 text-[11px] font-medium transition {profile.profile_id ===
            userProfiles.activeProfileId
              ? 'bg-surface-800 text-surface-50'
              : 'text-surface-400 hover:bg-surface-800/60 hover:text-surface-200'}"
            disabled={userProfiles.saving}
            onclick={() => void switchProfile(profile.profile_id)}
          >
            {profile.display_name}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  {#if identity.loading && entries.length === 0}
    <p class="px-3 py-4 text-[12px] text-surface-500">Loading…</p>
  {:else if identity.error && entries.length === 0}
    <p class="px-3 py-4 text-[12px] text-warning-400">{identity.error}</p>
  {:else if entries.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 px-3 py-6 text-center">
      <Brain size={22} strokeWidth={1.5} class="text-surface-500" />
      <p class="text-sm text-surface-300">Nothing remembered yet</p>
      <button
        type="button"
        class="btn btn-sm btn-primary"
        onclick={() => {
          onPickProfile?.(userProfiles.activeProfileId ?? "");
          dispatchProfilesFocusTeach();
        }}
      >
        Teach something
      </button>
    </div>
  {:else}
    <ul class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5">
      {#each entries as entry (entry.id)}
        <li>
          <button
            type="button"
            class="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-surface-800/70 {profilesSelection.selectedId ===
            entry.id
              ? 'bg-surface-800/90'
              : ''}"
            onclick={() => pickEntry(entry.id)}
          >
            <span class="min-w-0 flex-1">
              <span class="block truncate text-[13px] font-medium text-surface-100">
                {entry.title}
              </span>
              <span class="block truncate text-[11px] text-surface-500">
                {entry.subtitle}
              </span>
            </span>
            <span class="mt-0.5 shrink-0 text-[10px] uppercase tracking-wide text-surface-500">
              {profileKindLabel(entry.kind)}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
