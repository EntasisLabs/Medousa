<script lang="ts">
  import { Brain, UserPlus } from "@lucide/svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import {
    dispatchProfilesAddPerson,
    dispatchProfilesFocusTeach,
  } from "$lib/utils/profilesChromeEvents";
  import { onMount } from "svelte";

  interface Props {
    onPickProfile?: (profileId: string) => void;
    onOpenContext?: () => void;
    chrome?: "default" | "rail-list";
  }

  let { onPickProfile, onOpenContext, chrome = "rail-list" }: Props = $props();

  onMount(() => {
    if (userProfiles.profiles.length === 0) {
      void userProfiles.load();
    }
  });

  async function switchProfile(profileId: string) {
    await userProfiles.setActive(profileId);
    onPickProfile?.(profileId);
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-chrome={chrome}>
  {#if userProfiles.hasMultipleProfiles}
    <ul class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5">
      {#each userProfiles.profiles as profile (profile.profile_id)}
        <li>
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-surface-800/70 {profile.profile_id ===
            userProfiles.activeProfileId
              ? 'bg-surface-800/90 text-surface-50'
              : 'text-surface-200'}"
            disabled={userProfiles.saving}
            onclick={() => void switchProfile(profile.profile_id)}
          >
            <span class="min-w-0 flex-1 truncate text-[13px] font-medium">
              {profile.display_name}
            </span>
            {#if profile.profile_id === userProfiles.activeProfileId}
              <span class="text-[10px] uppercase tracking-wide text-primary-300">Active</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {:else}
    <ul class="min-h-0 flex-1 space-y-0.5 overflow-y-auto px-1.5 py-1.5">
      <li>
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-surface-200 transition hover:bg-surface-800/70"
          onclick={() => {
            dispatchProfilesAddPerson();
            onPickProfile?.(userProfiles.activeProfileId ?? "");
          }}
        >
          <UserPlus size={15} strokeWidth={1.75} class="shrink-0 text-surface-400" />
          <span class="text-[13px] font-medium">Add person</span>
        </button>
      </li>
      <li>
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-surface-200 transition hover:bg-surface-800/70"
          onclick={() => {
            dispatchProfilesFocusTeach();
            onPickProfile?.(userProfiles.activeProfileId ?? "");
          }}
        >
          <span class="text-[13px] font-medium">Teach something</span>
        </button>
      </li>
      <li>
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-surface-200 transition hover:bg-surface-800/70"
          onclick={() => onOpenContext?.()}
        >
          <Brain size={15} strokeWidth={1.75} class="shrink-0 text-surface-400" />
          <span class="text-[13px] font-medium">Open Context</span>
        </button>
      </li>
    </ul>
  {/if}
</div>
