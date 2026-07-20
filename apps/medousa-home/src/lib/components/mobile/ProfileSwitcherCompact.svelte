<script lang="ts">
  import { tick } from "svelte";
  import { ChevronDown, X } from "@lucide/svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { haptic } from "$lib/haptics";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeComposerPopover } from "$lib/utils/railPopover";

  interface Props {
    /** Hide when only the default profile exists (legacy pill row). */
    hideWhenSingle?: boolean;
    /** Show a dismissible footer chip when a non-Personal profile is active. */
    showChip?: boolean;
    open?: boolean;
    anchorEl?: HTMLElement | null;
  }

  let {
    hideWhenSingle = true,
    showChip = false,
    open = $bindable(false),
    anchorEl = null,
  }: Props = $props();

  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  const isNonDefault = $derived(
    userProfiles.activeDisplayName.trim().toLowerCase() !== "personal",
  );
  const chipVisible = $derived(showChip && isNonDefault);
  /** Legacy always-visible pill (mobile pill row) when not using footer chips. */
  const legacyPillVisible = $derived(
    !showChip && (!hideWhenSingle || userProfiles.hasMultipleProfiles),
  );

  $effect(() => {
    if (open && userProfiles.profiles.length === 0 && !userProfiles.loading) {
      void userProfiles.load();
    }
  });

  $effect(() => {
    if (!open || !menuEl) return;
    const anchor = (chipVisible || legacyPillVisible ? triggerEl : null) ?? anchorEl ?? triggerEl;
    if (!anchor) return;

    let frame = 0;
    const place = () => {
      if (!menuEl) return;
      placeComposerPopover(anchor, menuEl);
      frame = window.requestAnimationFrame(() => {
        if (menuEl) placeComposerPopover(anchor, menuEl);
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);

    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) =>
        Boolean(
          menuEl?.contains(target) ||
            triggerEl?.contains(target) ||
            anchor.contains(target),
        ),
      onDismiss: () => {
        open = false;
      },
    });

    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
      detachDismiss();
    };
  });

  async function pickProfile(profileId: string) {
    haptic("light");
    await userProfiles.setActive(profileId);
    open = false;
  }

  function openMenu() {
    if (userProfiles.saving) return;
    haptic("light");
    open = !open;
  }

  async function resetToPersonal(event: MouseEvent) {
    event.stopPropagation();
    event.preventDefault();
    const personal =
      userProfiles.profiles.find(
        (profile) => profile.display_name.trim().toLowerCase() === "personal",
      ) ?? userProfiles.profiles[0];
    if (personal) {
      await userProfiles.setActive(personal.profile_id);
    }
  }

  function openFullProfiles() {
    haptic("light");
    open = false;
    if (layout.isMobile) {
      layout.openMore("profiles");
      return;
    }
    shellTabs.openSurface("profiles", { activate: true });
  }
</script>

{#if chipVisible}
  <div class="composer-footer-chip-wrap">
    <button
      bind:this={triggerEl}
      type="button"
      class="composer-footer-chip"
      aria-label="Switch profile — {userProfiles.activeDisplayName}"
      aria-haspopup="dialog"
      aria-expanded={open}
      disabled={userProfiles.saving}
      onclick={openMenu}
    >
      <span class="composer-footer-chip-mono" aria-hidden="true">{userProfiles.profileMonogram}</span>
      <span class="truncate font-medium">{userProfiles.activeDisplayName}</span>
      <ChevronDown size={12} class="shrink-0 opacity-50" strokeWidth={2} />
    </button>
    <button
      type="button"
      class="composer-footer-chip-dismiss"
      aria-label="Switch to Personal"
      onclick={(event) => void resetToPersonal(event)}
    >
      <X size={12} strokeWidth={2} />
    </button>
  </div>
{:else if legacyPillVisible}
  <button
    bind:this={triggerEl}
    type="button"
    class="mobile-profile-pill shrink-0"
    aria-label="Switch profile — {userProfiles.activeDisplayName}"
    aria-haspopup="dialog"
    aria-expanded={open}
    disabled={userProfiles.saving}
    onclick={openMenu}
  >
    <span class="mobile-profile-monogram" aria-hidden="true">{userProfiles.profileMonogram}</span>
    <span class="max-w-[5.5rem] truncate text-xs font-medium text-surface-200">
      {userProfiles.activeDisplayName}
    </span>
    <ChevronDown size={14} class="shrink-0 text-surface-500" strokeWidth={2} />
  </button>
{/if}

{#if open}
  <div
    bind:this={menuEl}
    class="composer-anchored-menu"
    role="dialog"
    aria-label="Switch profile"
  >
    <header class="composer-anchored-menu-header">
      <div class="min-w-0">
        <h2 class="text-sm font-semibold text-surface-50">You · profile</h2>
        <p class="workshop-faint mt-0.5 text-xs">Work and home stay separate</p>
      </div>
    </header>

    <div class="composer-anchored-menu-body">
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
      <button
        type="button"
        class="workshop-text-action mt-3 text-sm"
        onclick={openFullProfiles}
      >
        Teach Medousa &amp; manage profiles →
      </button>
    </div>
  </div>
{/if}
