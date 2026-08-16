<script lang="ts">
  import "$lib/styles/profiles.postcss";
  import ProfilesAddPersonSheet from "$lib/components/profiles/ProfilesAddPersonSheet.svelte";
  import ProfilesFocusCard from "$lib/components/profiles/ProfilesFocusCard.svelte";
  import ProfilesIdentityField from "$lib/components/profiles/ProfilesIdentityField.svelte";
  import ProfilesOverflowMenu from "$lib/components/profiles/ProfilesOverflowMenu.svelte";
  import ProfilesTeachDialog from "$lib/components/profiles/ProfilesTeachDialog.svelte";
  import { getIdentityDigestPreview } from "$lib/daemon";
  import { identity } from "$lib/stores/identity.svelte";
  import { profilesSelection } from "$lib/stores/profilesSelection.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import type { IdentityRememberRequest, IdentityRememberResponse } from "$lib/types/identity";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { blobForShelfEntry, buildIdentityFieldLayout } from "$lib/utils/identityField";
  import {
    buildProfileShelfEntries,
    findShelfEntryAfterRemember,
    humanDigestLines,
  } from "$lib/utils/profileShelf";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import {
    PROFILES_ADD_PERSON_EVENT,
    PROFILES_FOCUS_TEACH_EVENT,
  } from "$lib/utils/profilesChromeEvents";
  import { onMount } from "svelte";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
    onOpenChat?: () => void;
  }

  let { visible, mobile = false, embedded = false, onOpenChat }: Props = $props();

  let digestLines = $state<string[]>([]);
  let personSheetOpen = $state(false);
  let teachDialogOpen = $state(false);
  let shelfNotice = $state<string | null>(null);
  let teachPrefill = $state("");

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const fieldLayout = $derived(
    buildIdentityFieldLayout(
      identity.context,
      userProfiles.activeDisplayName,
      digestLines,
    ),
  );

  const shelfEntries = $derived(
    identity.context ? buildProfileShelfEntries(identity.context) : [],
  );

  const selectedBlob = $derived.by(() => {
    const id = profilesSelection.selectedId;
    if (!id) return null;
    const fromField = fieldLayout.blobs.find((blob) => blob.id === id);
    if (fromField) return fromField;
    const entry = shelfEntries.find((row) => row.id === id);
    return entry ? blobForShelfEntry(entry) : null;
  });

  $effect(() => {
    if (visible) {
      void refreshField();
      if (!userProfiles.loading && userProfiles.profiles.length === 0) {
        void userProfiles.load();
      }
    }
  });

  $effect(() => {
    if (!mobile || !visible) return;
    return registerMobileBackHandler(() => {
      if (teachDialogOpen) {
        teachDialogOpen = false;
        return true;
      }
      if (personSheetOpen) {
        personSheetOpen = false;
        return true;
      }
      if (!profilesSelection.selectedId) return false;
      profilesSelection.select(null);
      return true;
    });
  });

  async function refreshField() {
    await identity.refresh({
      relationshipLimit: 32,
      userId: userProfiles.resolvedUserId,
    });
    try {
      const preview = await getIdentityDigestPreview({
        mode: "cognitive",
        relationship_limit: 32,
        user_id: userProfiles.resolvedUserId ?? undefined,
      });
      digestLines = humanDigestLines(preview.digest_text);
    } catch {
      digestLines = [];
    }
  }

  async function focusAfterRemember(
    parsed: IdentityRememberRequest,
    result?: IdentityRememberResponse,
  ) {
    await refreshField();
    if (result && !result.committed && result.requires_confirmation) {
      shelfNotice = result.message;
      return;
    }
    shelfNotice = null;
    const entries = identity.context ? buildProfileShelfEntries(identity.context) : [];
    const entry = findShelfEntryAfterRemember(entries, parsed);
    profilesSelection.select(entry?.id ?? null);
  }

  async function handleRemembered(
    parsed: IdentityRememberRequest,
    result: IdentityRememberResponse,
  ) {
    await focusAfterRemember(parsed, result);
  }

  function openTeach(prefill = "") {
    teachPrefill = prefill;
    teachDialogOpen = true;
  }

  function handleCorrect() {
    const blob = selectedBlob;
    if (!blob) return;
    if (blob.kind === "person") {
      openTeach(`${blob.label} is my ${blob.subtitle}`);
    } else if (blob.kind === "preference") {
      openTeach(`My ${blob.label.toLowerCase()} is ${blob.subtitle}`);
    } else {
      openTeach(blob.subtitle || blob.label);
    }
  }

  async function switchProfile(profileId: string) {
    await userProfiles.setActive(profileId);
    profilesSelection.select(null);
    await refreshField();
  }

  onMount(() => {
    const onAddPerson = () => {
      personSheetOpen = true;
    };
    const onFocusTeach = () => {
      openTeach("");
    };
    window.addEventListener(PROFILES_ADD_PERSON_EVENT, onAddPerson);
    window.addEventListener(PROFILES_FOCUS_TEACH_EVENT, onFocusTeach);
    return () => {
      window.removeEventListener(PROFILES_ADD_PERSON_EVENT, onAddPerson);
      window.removeEventListener(PROFILES_FOCUS_TEACH_EVENT, onFocusTeach);
    };
  });
</script>

<section
  class="profiles-panel flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'} {embedded
    ? 'profiles-panel-embedded'
    : ''}"
>
  {#if !embedded}
    <header class="workshop-header shrink-0">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div class="flex min-w-0 items-start gap-2">
          <ShellSidebarExpandButton label="Show rail" />
          <div class="min-w-0">
            <h1 class="text-base font-semibold text-surface-50">You</h1>
            <p class="workshop-header-line mt-1">
              Who she knows you as — feel the field, tap to focus
            </p>
          </div>
        </div>
        <ProfilesOverflowMenu {mobile} />
      </div>

      {#if userProfiles.profiles.length > 1}
        <div class="settings-profile-quick-row mt-3">
          {#each userProfiles.profiles as profile (profile.profile_id)}
            <button
              type="button"
              class="settings-profile-quick-btn {profile.profile_id === userProfiles.activeProfileId
                ? 'settings-profile-quick-btn-active'
                : ''}"
              disabled={userProfiles.saving || readOnly}
              onclick={() => void switchProfile(profile.profile_id)}
            >
              {profile.display_name}
            </button>
          {/each}
        </div>
      {/if}

      {#if shelfNotice}
        <p class="mt-2 text-xs text-content-warning" role="status">{shelfNotice}</p>
      {/if}
    </header>
  {:else}
    <header class="shrink-0 border-b border-surface-500/40 px-4 py-2">
      <div class="flex items-center justify-end gap-1">
        <ProfilesOverflowMenu {mobile} />
      </div>
    </header>
  {/if}

  <div class="relative min-h-0 flex-1">
    <ProfilesIdentityField
      layout={fieldLayout}
      selectedId={profilesSelection.selectedId}
      loading={identity.loading}
      onSelect={(blob) => {
        profilesSelection.select(blob?.id ?? null);
      }}
    />

    <ProfilesFocusCard
      blob={selectedBlob}
      portrait={fieldLayout.portrait}
      onClose={() => {
        profilesSelection.select(null);
      }}
      onOpenChat={onOpenChat}
      onCorrect={selectedBlob ? handleCorrect : undefined}
    />
  </div>

  <ProfilesTeachDialog
    open={teachDialogOpen}
    {readOnly}
    prefill={teachPrefill}
    onClose={() => {
      teachDialogOpen = false;
      teachPrefill = "";
    }}
    onRemembered={handleRemembered}
  />

  <ProfilesAddPersonSheet
    open={personSheetOpen}
    {readOnly}
    onClose={() => {
      personSheetOpen = false;
    }}
    onSaved={(parsed) => focusAfterRemember(parsed)}
  />
</section>
