<script lang="ts">
  import { Check, Plus } from "@lucide/svelte";
  import WorkshopJoinSheet from "$lib/components/workshops/WorkshopJoinSheet.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { haptic } from "$lib/haptics";
  import type { WorkshopServer } from "$lib/types/workshopRegistry";
  import {
    workshopBrandCssVars,
    workshopHostLabel,
    workshopMonogram,
    workshopRemoteAccessNote,
  } from "$lib/types/workshopRegistry";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { isTauri } from "$lib/window";

  interface Props {
    hideWhenSingle?: boolean;
    variant?: "mobile" | "desktop" | "rail";
  }

  let { hideWhenSingle = true, variant = "mobile" }: Props = $props();

  let sheetOpen = $state(false);
  let joinOpen = $state(false);

  const showPill = $derived(
    variant !== "rail" &&
      isTauri() &&
      (!hideWhenSingle || workshops.hasMultipleWorkshops),
  );

  const showRail = $derived(variant === "rail");
  const isRailMenu = $derived(variant === "rail");

  const activeBrandStyle = $derived(workshopBrandCssVars(workshops.activeWorkshop?.brandColor));

  async function pickWorkshop(workshopId: string) {
    haptic("light");
    workshops.requestSwitch(workshopId);
    if (!workshops.confirmSwitchId) {
      sheetOpen = false;
    }
  }

  function openSheet() {
    if (workshops.switching) return;
    haptic("light");
    sheetOpen = true;
    if (workshops.workshops.length === 0 && !workshops.loading) {
      void workshops.load();
    }
  }

  function openConnectionSettings() {
    haptic("light");
    sheetOpen = false;
    if (variant === "mobile") {
      layout.openMore("settings");
    } else {
      settingsNav.openSection("basement");
      layout.navigateDesktop("settings", { bump: true });
    }
  }

  function workshopMeta(workshop: WorkshopServer): string {
    const remoteNote = workshopRemoteAccessNote(workshop, isTauriMobilePlatform());
    const host = workshopHostLabel(workshop.url, workshop.kind);
    if (workshop.id === workshops.activeWorkshopId) {
      if (connection.checking) return "Connecting…";
      if (connection.online) {
        return `Connected · ${remoteNote ?? host}`;
      }
      if (connection.offline) {
        return `Offline · ${remoteNote ?? host}`;
      }
    }
    if (workshop.kind === "local") return host;
    return remoteNote ?? host;
  }

  function avatarStyle(workshop: WorkshopServer): string | undefined {
    return workshopBrandCssVars(workshop.brandColor);
  }
</script>

{#if showRail}
  <button
    type="button"
    class="workshop-rail-btn workshop-rail-workshop-btn mb-9 font-semibold leading-none {sheetOpen
      ? 'workshop-rail-workshop-btn-open'
      : ''}"
    style={activeBrandStyle}
    title="Switch workshop — {workshops.activeLabel}"
    aria-label="Switch workshop — {workshops.activeLabel}"
    aria-haspopup="menu"
    aria-expanded={sheetOpen}
    disabled={workshops.switching}
    onclick={openSheet}
  >
    <span class="workshop-rail-workshop-monogram" aria-hidden="true">
      {workshops.activeMonogram}
    </span>
  </button>
{:else if showPill}
  <button
    type="button"
    class="{variant === 'mobile'
      ? 'mobile-profile-pill shrink-0'
      : 'flex max-w-[9rem] shrink-0 items-center gap-1.5 rounded-lg border border-surface-500/35 bg-surface-900/60 px-2 py-1 text-surface-200 transition hover:border-surface-400/40 hover:bg-surface-800/70'}"
    aria-label="Switch workshop — {workshops.activeLabel}"
    aria-haspopup="menu"
    aria-expanded={sheetOpen}
    disabled={workshops.switching}
    onclick={openSheet}
  >
    <span
      class="{variant === 'mobile'
        ? 'mobile-profile-monogram'
        : 'workshop-switcher-avatar h-5 w-5 text-[10px]'}"
      style={activeBrandStyle}
      aria-hidden="true"
    >
      {workshops.activeMonogram}
    </span>
    <span
      class="truncate text-xs font-medium text-surface-200 {variant === 'mobile'
        ? 'max-w-[5.5rem]'
        : 'max-w-[6rem]'}"
    >
      {workshops.activeLabel}
    </span>
  </button>
{/if}

{#if workshops.pendingSwitchAfterPair}
  <div
    class="mobile-sheet-backdrop {isRailMenu ? 'workshop-rail-sheet-backdrop' : ''}"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) workshops.dismissSwitchAfterPair();
    }}
  >
    <div
      class="mobile-sheet max-w-sm {isRailMenu ? 'workshop-rail-sheet' : ''}"
      role="alertdialog"
      aria-label="Switch to new workshop?"
    >
      <header class="mobile-sheet-header">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold text-surface-50">
            Switch to {workshops.pendingSwitchAfterPairLabel}?
          </h2>
          <p class="workshop-faint mt-0.5 text-xs leading-relaxed">
            You're connected. Switch now, or stay on your current workshop.
          </p>
        </div>
      </header>
      <div class="flex flex-wrap gap-2 px-4 pb-6 pt-2">
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={workshops.switching}
          onclick={() => {
            sheetOpen = false;
            void workshops.confirmSwitchAfterPair();
          }}
        >
          Switch now
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => workshops.dismissSwitchAfterPair()}
        >
          Later
        </button>
      </div>
    </div>
  </div>
{/if}

{#if workshops.confirmSwitchId}
  <div
    class="mobile-sheet-backdrop {isRailMenu ? 'workshop-rail-sheet-backdrop' : ''}"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) workshops.cancelSwitchConfirm();
    }}
  >
    <div
      class="mobile-sheet max-w-sm {isRailMenu ? 'workshop-rail-sheet' : ''}"
      role="alertdialog"
      aria-label="Switch workshop?"
    >
      <header class="mobile-sheet-header">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold text-surface-50">Switch workshop?</h2>
          <p class="workshop-faint mt-0.5 text-xs leading-relaxed">
            Unsaved vault edits or a live turn may be interrupted if you switch now.
          </p>
        </div>
      </header>
      <div class="flex flex-wrap gap-2 px-4 pb-6 pt-2">
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={workshops.switching}
          onclick={() => workshops.confirmSwitch()}
        >
          {workshops.switching ? "Switching…" : "Switch anyway"}
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => workshops.cancelSwitchConfirm()}
        >
          Stay here
        </button>
      </div>
    </div>
  </div>
{/if}

{#if sheetOpen}
  <div
    class="mobile-sheet-backdrop {isRailMenu ? 'workshop-rail-sheet-backdrop' : ''}"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) sheetOpen = false;
    }}
  >
    <div
      class="{isRailMenu ? 'workshop-rail-sheet workshop-switcher-menu' : 'mobile-sheet'}"
      role="menu"
      aria-label="Switch workshop"
    >
      <header class="{isRailMenu ? 'workshop-switcher-header' : 'mobile-sheet-header'}">
        <div class="min-w-0">
          <h2 class="{isRailMenu ? 'workshop-switcher-title' : 'text-sm font-semibold text-surface-50'}">
            Workshops
          </h2>
          {#if !isRailMenu}
            <p class="workshop-faint mt-0.5 text-xs">Switch between your workshops</p>
          {/if}
        </div>
        {#if !isRailMenu}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface shrink-0"
            onclick={() => {
              sheetOpen = false;
            }}
          >
            Done
          </button>
        {/if}
      </header>

      <div class="{isRailMenu ? 'workshop-switcher-list' : 'mobile-you-scroll px-4 pb-6 pt-2'}">
        {#if workshops.loading && workshops.workshops.length === 0}
          <p class="workshop-faint px-2 text-sm">Loading…</p>
        {:else if workshops.error}
          <p class="px-2 text-sm text-error-400">{workshops.error}</p>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface mx-2 mt-3"
            onclick={() => workshops.load()}
          >
            Retry
          </button>
        {:else}
          {#each workshops.workshops as workshop (workshop.id)}
            {@const isActive = workshop.id === workshops.activeWorkshopId}
            <button
              type="button"
              role="menuitemradio"
              aria-checked={isActive}
              class="workshop-switcher-row {isActive ? 'workshop-switcher-row-active' : ''}"
              disabled={workshops.switching}
              onclick={() => pickWorkshop(workshop.id)}
            >
              <span
                class="workshop-switcher-avatar"
                style={avatarStyle(workshop)}
                aria-hidden="true"
              >
                {workshopMonogram(workshop.label)}
              </span>
              <span class="workshop-switcher-row-body">
                <span class="workshop-switcher-row-name">{workshop.label}</span>
                <span class="workshop-switcher-row-meta">
                  {#if workshop.tagline}
                    {workshop.tagline}
                  {:else}
                    {workshopMeta(workshop)}
                  {/if}
                </span>
              </span>
              {#if isActive}
                <Check size={16} strokeWidth={2.5} class="workshop-switcher-row-check" aria-hidden="true" />
              {/if}
            </button>
          {/each}
        {/if}
      </div>

      {#if !workshops.loading && !workshops.error}
        <div class="{isRailMenu ? 'workshop-switcher-footer' : 'px-4 pb-4'}">
          <button
            type="button"
            role="menuitem"
            class="{isRailMenu ? 'workshop-switcher-action' : 'btn btn-sm variant-soft-primary mt-4 w-full'}"
            disabled={workshops.atWorkshopLimit}
            onclick={() => {
              joinOpen = true;
            }}
          >
            {#if isRailMenu}
              <span class="workshop-switcher-action-icon" aria-hidden="true">
                <Plus size={14} strokeWidth={2} />
              </span>
            {:else}
              <Plus class="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
            {/if}
            Add a workshop
          </button>
          <button
            type="button"
            role="menuitem"
            class="{isRailMenu
              ? 'workshop-switcher-manage'
              : 'workshop-text-action mt-3 text-sm'}"
            onclick={openConnectionSettings}
          >
            Manage in Settings
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<WorkshopJoinSheet
  open={joinOpen}
  {variant}
  onClose={() => {
    joinOpen = false;
  }}
  onJoined={() => {
    sheetOpen = false;
  }}
/>
