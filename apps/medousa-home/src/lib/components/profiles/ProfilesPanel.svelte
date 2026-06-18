<script lang="ts">
  import ProfilesAddPersonSheet from "$lib/components/profiles/ProfilesAddPersonSheet.svelte";
  import ProfilesOverflowMenu from "$lib/components/profiles/ProfilesOverflowMenu.svelte";
  import ProfilesShelfDetail from "$lib/components/profiles/ProfilesShelfDetail.svelte";
  import ProfilesShelfList from "$lib/components/profiles/ProfilesShelfList.svelte";
  import ProfilesTeachComposer from "$lib/components/profiles/ProfilesTeachComposer.svelte";
  import { getIdentityDigestPreview } from "$lib/daemon";
  import { identity } from "$lib/stores/identity.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import type { IdentityRememberRequest, IdentityRememberResponse } from "$lib/types/identity";
  import type { ProfileShelfFilter } from "$lib/types/profileShelf";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { isTauriMobilePlatform } from "$lib/platform";
  import {
    buildProfileShelfEntries,
    filterProfileShelfEntries,
    findShelfEntryAfterRemember,
    humanDigestLines,
    shelfTabForRemember,
  } from "$lib/utils/profileShelf";
  import { UserPlus } from "@lucide/svelte";
  import { ChevronLeft } from "@lucide/svelte";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
    onOpenChat?: () => void;
  }

  let { visible, mobile = false, embedded = false, onOpenChat }: Props = $props();

  const SHELF_TABS: { id: ProfileShelfFilter; label: string }[] = [
    { id: "all", label: "All" },
    { id: "people", label: "People" },
    { id: "facts", label: "Facts" },
  ];

  let activeTab = $state<ProfileShelfFilter>("all");
  let search = $state("");
  let selectedId = $state<string | null>(null);
  let mobileDetailOpen = $state(false);
  let digestLines = $state<string[]>([]);
  let personSheetOpen = $state(false);
  let shelfNotice = $state<string | null>(null);

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const shelfEntries = $derived(
    identity.context ? buildProfileShelfEntries(identity.context) : [],
  );

  const filteredEntries = $derived(
    filterProfileShelfEntries(shelfEntries, search, activeTab),
  );

  const selectedEntry = $derived(
    selectedId
      ? (filteredEntries.find((entry) => entry.id === selectedId) ??
        shelfEntries.find((entry) => entry.id === selectedId) ??
        null)
      : null,
  );

  const mobileDetailLabel = $derived(selectedEntry?.title ?? "Memory");

  $effect(() => {
    if (visible) {
      void refreshShelf();
      if (!userProfiles.loading && userProfiles.profiles.length === 0) {
        void userProfiles.load();
      }
    }
  });

  $effect(() => {
    if (!mobile || !visible) return;
    return registerMobileBackHandler(() => {
      if (!mobileDetailOpen) return false;
      mobileDetailOpen = false;
      return true;
    });
  });

  $effect(() => {
    activeTab;
    search;
    if (filteredEntries.length === 0) {
      selectedId = null;
      mobileDetailOpen = false;
      return;
    }
    if (selectedId && filteredEntries.some((entry) => entry.id === selectedId)) {
      return;
    }
    if (!mobile) {
      selectedId = filteredEntries[0]?.id ?? null;
    } else if (selectedId && !filteredEntries.some((entry) => entry.id === selectedId)) {
      selectedId = null;
      mobileDetailOpen = false;
    }
  });

  async function refreshShelf() {
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
    await refreshShelf();
    if (result && !result.committed && result.requires_confirmation) {
      shelfNotice = result.message;
      return;
    }
    shelfNotice = null;
    activeTab = shelfTabForRemember(parsed.fact_kind);
    const entries = identity.context ? buildProfileShelfEntries(identity.context) : [];
    const entry = findShelfEntryAfterRemember(entries, parsed);
    if (entry) {
      selectedId = entry.id;
      if (mobile) mobileDetailOpen = true;
    }
  }

  async function handleRemembered(
    parsed: IdentityRememberRequest,
    result: IdentityRememberResponse,
  ) {
    await focusAfterRemember(parsed, result);
  }

  function selectEntry(id: string) {
    selectedId = id;
    if (mobile) mobileDetailOpen = true;
  }

  async function switchProfile(profileId: string) {
    await userProfiles.setActive(profileId);
    selectedId = null;
    mobileDetailOpen = false;
    await refreshShelf();
  }

  function setTab(tab: ProfileShelfFilter) {
    activeTab = tab;
    selectedId = null;
    mobileDetailOpen = false;
  }
</script>

<section
  class="profiles-panel flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'} {embedded
    ? 'profiles-panel-embedded'
    : ''}"
>
  {#if !mobile || !mobileDetailOpen}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      {#if !embedded}
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div class="min-w-0">
            <h1 class="text-base font-semibold text-surface-50">Profiles</h1>
            <p class="workshop-header-line mt-1">
              Who she knows you as — memories she carries into every thread
            </p>
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
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              disabled={identity.loading}
              onclick={() => void refreshShelf()}
            >
              {identity.loading ? "Refreshing…" : "Refresh"}
            </button>
          </div>
        </div>
      {:else}
        <div class="flex items-center justify-end gap-1">
          <ProfilesOverflowMenu {mobile} />
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={identity.loading}
            onclick={() => void refreshShelf()}
          >
            {identity.loading ? "…" : "Refresh"}
          </button>
        </div>
      {/if}

      {#if userProfiles.profiles.length > 0}
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

      <div class="workshop-tabs mt-3">
        {#each SHELF_TABS as tab (tab.id)}
          <button
            type="button"
            class="workshop-tab {activeTab === tab.id ? 'workshop-tab-active' : ''}"
            onclick={() => setTab(tab.id)}
          >
            {tab.label}
          </button>
        {/each}
      </div>

      <label class="mt-3 block">
        <span class="sr-only">Search memories</span>
        <input
          class="input w-full {embedded ? '' : 'max-w-lg'} text-sm"
          type="search"
          placeholder="Search people, facts, preferences…"
          bind:value={search}
        />
      </label>
      {#if shelfNotice}
        <p class="mt-2 text-xs text-warning-400" role="status">{shelfNotice}</p>
      {/if}
    </header>
  {:else if mobile && mobileDetailOpen}
    <div class="flex items-center gap-2 border-b border-surface-500/40 px-4 py-2">
      <button
        type="button"
        class="mobile-icon-btn shrink-0"
        aria-label="Back to shelf"
        onclick={() => {
          mobileDetailOpen = false;
        }}
      >
        <ChevronLeft size={20} strokeWidth={1.75} />
      </button>
      <p class="workshop-faint truncate text-xs">{mobileDetailLabel}</p>
    </div>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    {#if !mobile || !mobileDetailOpen}
      <aside
        class="workshop-list-pane mobile-you-scroll min-w-0 shrink-0 overflow-y-auto px-3 py-3 {mobile
          ? 'w-full'
          : 'w-[min(300px,34%)] border-r border-surface-500/40'}"
      >
        <ProfilesShelfList
          entries={filteredEntries}
          selectedId={selectedId}
          loading={identity.loading}
          error={identity.error}
          profileLabel={userProfiles.activeDisplayName}
          onSelect={selectEntry}
        />
      </aside>
    {/if}

    {#if !mobile || mobileDetailOpen}
      <div
        class="workshop-detail-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-4 {mobile
          ? ''
          : 'max-w-2xl'}"
      >
        <ProfilesShelfDetail
          entry={selectedEntry}
          {digestLines}
          {readOnly}
          onOpenChat={onOpenChat}
          onAddPerson={() => {
            personSheetOpen = true;
          }}
          onUpdated={(parsed) => (parsed ? focusAfterRemember(parsed) : refreshShelf())}
        />
      </div>
    {/if}
  </div>

  {#if !mobile || !mobileDetailOpen}
    <ProfilesTeachComposer {readOnly} onRemembered={handleRemembered} />
  {/if}

  <ProfilesAddPersonSheet
    open={personSheetOpen}
    {readOnly}
    onClose={() => {
      personSheetOpen = false;
    }}
    onSaved={(parsed) => focusAfterRemember(parsed)}
  />
</section>
