<script lang="ts">
  import ProfilesAddPersonSheet from "$lib/components/profiles/ProfilesAddPersonSheet.svelte";
  import ProfilesFocusCard from "$lib/components/profiles/ProfilesFocusCard.svelte";
  import ProfilesIdentityField from "$lib/components/profiles/ProfilesIdentityField.svelte";
  import ProfilesOverflowMenu from "$lib/components/profiles/ProfilesOverflowMenu.svelte";
  import ProfilesTeachComposer from "$lib/components/profiles/ProfilesTeachComposer.svelte";
  import { getIdentityDigestPreview } from "$lib/daemon";
  import { identity } from "$lib/stores/identity.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import type { IdentityRememberRequest, IdentityRememberResponse } from "$lib/types/identity";
  import type { IdentityFieldBlob } from "$lib/types/identityField";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { buildIdentityFieldLayout } from "$lib/utils/identityField";
  import {
    buildProfileShelfEntries,
    findShelfEntryAfterRemember,
    humanDigestLines,
  } from "$lib/utils/profileShelf";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import { UserPlus } from "@lucide/svelte";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
    onOpenChat?: () => void;
  }

  let { visible, mobile = false, embedded = false, onOpenChat }: Props = $props();

  let selectedBlob = $state<IdentityFieldBlob | null>(null);
  let digestLines = $state<string[]>([]);
  let personSheetOpen = $state(false);
  let shelfNotice = $state<string | null>(null);
  let teachPrefill = $state("");
  let teachFocusNonce = $state(0);

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const fieldLayout = $derived(
    buildIdentityFieldLayout(
      identity.context,
      userProfiles.activeDisplayName,
      digestLines,
    ),
  );

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
      if (!selectedBlob) return false;
      selectedBlob = null;
      return true;
    });
  });

  async function refreshField() {
    await identity.refresh({ relationshipLimit: 32 });
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
    if (entry) {
      const layout = buildIdentityFieldLayout(
        identity.context,
        userProfiles.activeDisplayName,
        digestLines,
      );
      selectedBlob = layout.blobs.find((b) => b.id === entry.id) ?? null;
    }
  }

  async function handleRemembered(
    parsed: IdentityRememberRequest,
    result: IdentityRememberResponse,
  ) {
    await focusAfterRemember(parsed, result);
  }

  function handleCorrect() {
    if (!selectedBlob) return;
    if (selectedBlob.kind === "person") {
      teachPrefill = `${selectedBlob.label} is my ${selectedBlob.subtitle}`;
    } else if (selectedBlob.kind === "preference") {
      teachPrefill = `My ${selectedBlob.label.toLowerCase()} is ${selectedBlob.subtitle}`;
    } else {
      teachPrefill = selectedBlob.subtitle || selectedBlob.label;
    }
    teachFocusNonce += 1;
  }

  async function switchProfile(profileId: string) {
    await userProfiles.setActive(profileId);
    selectedBlob = null;
    await refreshField();
  }
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
        <div class="flex shrink-0 items-center gap-1">
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            disabled={readOnly}
            onclick={() => {
              personSheetOpen = true;
            }}
          >
            <UserPlus size={14} class="mr-1" aria-hidden="true" />
            Person
          </button>
          <ProfilesOverflowMenu {mobile} />
        </div>
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
        <p class="mt-2 text-xs text-warning-400" role="status">{shelfNotice}</p>
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
      selectedId={selectedBlob?.id ?? null}
      loading={identity.loading}
      onSelect={(blob) => {
        selectedBlob = blob;
      }}
    />

    <ProfilesFocusCard
      blob={selectedBlob}
      portrait={fieldLayout.portrait}
      onClose={() => {
        selectedBlob = null;
      }}
      onOpenChat={onOpenChat}
      onCorrect={selectedBlob ? handleCorrect : undefined}
    />
  </div>

  <ProfilesTeachComposer
    {readOnly}
    prefill={teachPrefill}
    focusNonce={teachFocusNonce}
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
