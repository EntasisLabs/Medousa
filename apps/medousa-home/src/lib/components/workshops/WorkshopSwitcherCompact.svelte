<script lang="ts">
  import { Building2, ChevronDown, Home, Plus } from "@lucide/svelte";
  import WorkshopJoinSheet from "$lib/components/workshops/WorkshopJoinSheet.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { haptic } from "$lib/haptics";
  import type { WorkshopIcon, WorkshopServer } from "$lib/types/workshopRegistry";
  import { isTauri } from "$lib/window";

  interface Props {
    /** Hide pill when only one workshop (ignored for `rail`). */
    hideWhenSingle?: boolean;
    /** Mobile sheet, desktop header pill, or left-rail Slack-style monogram. */
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

  function workshopIcon(icon: WorkshopIcon | undefined) {
    if (icon === "building" || icon === "team") return Building2;
    return Home;
  }

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
      layout.setMobileTab("you");
      layout.openYou("settings");
    } else {
      settingsNav.openSection("basement");
      layout.navigateDesktop("settings", { bump: true });
    }
  }

  function kindLabel(workshop: WorkshopServer): string {
    return workshop.kind === "local" ? "This device" : "Paired";
  }

  function connectionDotClass(workshopId: string): string {
    if (workshopId !== workshops.activeWorkshopId) return "workshop-status-dot-muted";
    if (connection.online) return "workshop-status-dot-live";
    if (connection.offline) return "workshop-status-dot-warning";
    return "workshop-status-dot-muted";
  }
</script>

{#if showRail}
  <button
    type="button"
    class="workshop-rail-btn workshop-rail-workshop-btn mb-3 font-semibold leading-none"
    title="Switch workshop — {workshops.activeLabel}"
    aria-label="Switch workshop — {workshops.activeLabel}"
    aria-haspopup="dialog"
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
    aria-haspopup="dialog"
    aria-expanded={sheetOpen}
    disabled={workshops.switching}
    onclick={openSheet}
  >
    <span
      class="{variant === 'mobile'
        ? 'mobile-profile-monogram'
        : 'flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-surface-700/80 text-[10px] font-semibold text-surface-100'}"
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
    <ChevronDown size={14} class="shrink-0 text-surface-500" strokeWidth={2} />
  </button>
{/if}

{#if workshops.pendingSwitchAfterPair}
  <div
    class="mobile-sheet-backdrop {variant === 'rail' ? 'workshop-rail-sheet-backdrop' : ''}"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) workshops.dismissSwitchAfterPair();
    }}
  >
    <div
      class="mobile-sheet max-w-sm {variant === 'rail' ? 'workshop-rail-sheet' : ''}"
      role="alertdialog"
      aria-label="Switch to new workshop?"
    >
      <header class="mobile-sheet-header">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold text-surface-50">Switch to {workshops.pendingSwitchAfterPairLabel}?</h2>
          <p class="workshop-faint mt-0.5 text-xs leading-relaxed">
            Workshop joined. Switch now to talk to that engine, or stay on your current one.
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
    class="mobile-sheet-backdrop {variant === 'rail' ? 'workshop-rail-sheet-backdrop' : ''}"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) workshops.cancelSwitchConfirm();
    }}
  >
    <div
      class="mobile-sheet max-w-sm {variant === 'rail' ? 'workshop-rail-sheet' : ''}"
      role="alertdialog"
      aria-label="Switch workshop?"
    >
      <header class="mobile-sheet-header">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold text-surface-50">Switch workshop?</h2>
          <p class="workshop-faint mt-0.5 text-xs leading-relaxed">
            You have unsaved vault edits or a live turn. Switching reconnects to a different engine
            and may interrupt it.
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
          disabled={workshops.switching}
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
    class="mobile-sheet-backdrop {variant === 'rail' ? 'workshop-rail-sheet-backdrop' : ''}"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) sheetOpen = false;
    }}
  >
    <div
      class="mobile-sheet {variant === 'rail' ? 'workshop-rail-sheet' : ''}"
      role="dialog"
      aria-label="Switch workshop"
    >
      <header class="mobile-sheet-header">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold text-surface-50">Workshop</h2>
          <p class="workshop-faint mt-0.5 text-xs">Which engine Medousa talks to</p>
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
        {#if workshops.loading && workshops.workshops.length === 0}
          <p class="workshop-faint text-sm">Loading workshops…</p>
        {:else if workshops.error}
          <p class="text-sm text-error-400">{workshops.error}</p>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface mt-3"
            onclick={() => workshops.load()}
          >
            Retry
          </button>
        {:else}
          <div class="space-y-2">
            {#each workshops.workshops as workshop (workshop.id)}
              {@const Icon = workshopIcon(workshop.icon)}
              <button
                type="button"
                class="flex w-full items-center gap-3 rounded-xl border px-3 py-2.5 text-left transition {workshop.id ===
                workshops.activeWorkshopId
                  ? 'border-primary-500/40 bg-primary-500/10'
                  : 'border-surface-500/30 bg-surface-950/40 hover:border-surface-400/35 hover:bg-surface-900/50'}"
                disabled={workshops.switching}
                onclick={() => pickWorkshop(workshop.id)}
              >
                <span
                  class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-800/80 text-surface-200"
                  aria-hidden="true"
                >
                  <Icon size={16} strokeWidth={1.75} />
                </span>
                <span class="min-w-0 flex-1">
                  <span class="flex items-center gap-2">
                    <span class="{connectionDotClass(workshop.id)} shrink-0" aria-hidden="true"></span>
                    <span class="truncate text-sm font-medium text-surface-50">
                      {workshop.label}
                    </span>
                  </span>
                  <span class="workshop-faint block truncate text-xs">
                    {kindLabel(workshop)} · {workshop.url.replace(/^https?:\/\//, "")}
                  </span>
                </span>
                {#if workshop.id === workshops.activeWorkshopId}
                  <span class="badge variant-soft-primary shrink-0 text-[10px]">Active</span>
                {/if}
              </button>
            {/each}
          </div>
          <button
            type="button"
            class="btn btn-sm variant-soft-primary mt-4 w-full"
            disabled={workshops.atWorkshopLimit}
            onclick={() => {
              joinOpen = true;
            }}
          >
            <Plus class="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
            Add workshop
          </button>
        {/if}
        <button
          type="button"
          class="workshop-text-action mt-4 text-sm"
          onclick={openConnectionSettings}
        >
          Manage workshops in Settings →
        </button>
      </div>
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
