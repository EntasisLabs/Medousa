<script lang="ts">
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { contextShell } from "$lib/stores/contextShell.svelte";
  import { contextThreads } from "$lib/stores/contextThreads.svelte";
  import {
    placeDockPopover,
    type DockPopoverPlacement,
  } from "$lib/utils/dockPopoverPlace";
  import { AVEC_DIMENSIONS } from "$lib/utils/contextPosture";
  import {
    buildContextThreadEntries,
    filterContextThreadEntries,
  } from "$lib/utils/contextThreads";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";
  import { Filter, ListTree, Search, X } from "@lucide/svelte";
  import { tick } from "svelte";

  interface Props {
    variant?: "popover" | "rail-row";
    /** Ensure Map center surface is active after a pick. */
    onPick?: () => void;
  }

  let { variant: _variant = "popover", onPick }: Props = $props();

  let searchOpen = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  let momentsOpen = $state(false);
  let momentsBtnEl = $state<HTMLButtonElement | null>(null);
  let momentsMenuEl = $state<HTMLDivElement | null>(null);
  let momentsPlacement = $state<DockPopoverPlacement | null>(null);
  let momentsQuery = $state("");
  let momentsSearchEl = $state<HTMLInputElement | null>(null);

  let filterOpen = $state(false);
  let filterBtnEl = $state<HTMLButtonElement | null>(null);
  let filterMenuEl = $state<HTMLDivElement | null>(null);
  let filterPlacement = $state<DockPopoverPlacement | null>(null);

  const query = $derived(contextShell.search);
  const avecMins = $derived(contextShell.mapAvecMins);
  const filterActive = $derived(contextShell.mapAvecFilterActive);

  const sessionLabels = $derived(
    Object.fromEntries(
      chat.sessions.map((session) => [
        session.session_id,
        session.display_name?.trim() || session.session_id,
      ]),
    ),
  );

  const momentEntries = $derived(
    filterContextThreadEntries(
      buildContextThreadEntries(contextThreads.nodes, sessionLabels),
      momentsQuery,
    ).slice(0, 80),
  );

  async function openSearch() {
    closeMoments();
    closeFilter();
    await ensureRailPopoverOpen();
    searchOpen = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchOpen = false;
    contextShell.search = "";
  }

  function placeMoments() {
    if (!momentsBtnEl) return;
    momentsPlacement = placeDockPopover(momentsBtnEl, {
      preferUp: true,
      width: 280,
      maxHeight: 360,
    });
  }

  function placeFilter() {
    if (!filterBtnEl) return;
    filterPlacement = placeDockPopover(filterBtnEl, {
      preferUp: true,
      width: 240,
      maxHeight: 320,
    });
  }

  async function openMoments() {
    closeFilter();
    closeSearch();
    await ensureRailPopoverOpen();
    if (contextThreads.nodes.length === 0) {
      void contextThreads.refresh({ limit: 200 });
    }
    momentsOpen = true;
    requestAnimationFrame(placeMoments);
    await tick();
    momentsSearchEl?.focus();
  }

  function closeMoments() {
    momentsOpen = false;
    momentsPlacement = null;
    momentsQuery = "";
  }

  async function openFilter() {
    closeMoments();
    closeSearch();
    await ensureRailPopoverOpen();
    filterOpen = true;
    requestAnimationFrame(placeFilter);
  }

  function closeFilter() {
    filterOpen = false;
    filterPlacement = null;
  }

  function pickMoment(syncKey: string, sessionId: string) {
    contextShell.focusMapMoment(syncKey, sessionId);
    closeMoments();
    onPick?.();
  }

  function onWindowPointerDown(event: PointerEvent) {
    const target = event.target as Node;
    if (momentsOpen) {
      if (momentsBtnEl?.contains(target) || momentsMenuEl?.contains(target)) return;
      closeMoments();
    }
    if (filterOpen) {
      if (filterBtnEl?.contains(target) || filterMenuEl?.contains(target)) return;
      closeFilter();
    }
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    if (momentsOpen) {
      event.preventDefault();
      closeMoments();
    } else if (filterOpen) {
      event.preventDefault();
      closeFilter();
    }
  }

  function onWindowReposition() {
    if (momentsOpen) placeMoments();
    if (filterOpen) placeFilter();
  }

  $effect(() => {
    if (!momentsOpen && !filterOpen) return;
    window.addEventListener("pointerdown", onWindowPointerDown, true);
    window.addEventListener("keydown", onWindowKeydown);
    window.addEventListener("resize", onWindowReposition);
    window.addEventListener("scroll", onWindowReposition, true);
    return () => {
      window.removeEventListener("pointerdown", onWindowPointerDown, true);
      window.removeEventListener("keydown", onWindowKeydown);
      window.removeEventListener("resize", onWindowReposition);
      window.removeEventListener("scroll", onWindowReposition, true);
    };
  });
</script>

{#if searchOpen}
  <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
    <Search size={14} strokeWidth={1.75} class="shrink-0 text-surface-500" aria-hidden="true" />
    <input
      bind:this={searchInputEl}
      class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-surface-500 focus:outline-none focus:ring-0"
      type="search"
      placeholder="Search map…"
      value={query}
      oninput={(event) => {
        contextShell.search = (event.currentTarget as HTMLInputElement).value;
      }}
    />
    <button
      type="button"
      class="vault-dock-icon-btn"
      title="Close search"
      aria-label="Close search"
      onclick={closeSearch}
    >
      <X size={14} strokeWidth={1.75} />
    </button>
  </div>
{:else}
  <div class="min-w-0 flex-1" aria-hidden="true"></div>
  <button
    bind:this={momentsBtnEl}
    type="button"
    class="vault-dock-icon-btn"
    class:vault-dock-icon-btn-active={momentsOpen}
    title="Moments"
    aria-label="Browse moments"
    aria-haspopup="listbox"
    aria-expanded={momentsOpen}
    onclick={() => void (momentsOpen ? closeMoments() : openMoments())}
  >
    <ListTree size={15} strokeWidth={1.75} />
  </button>
  <button
    bind:this={filterBtnEl}
    type="button"
    class="vault-dock-icon-btn"
    class:vault-dock-icon-btn-active={filterOpen || filterActive}
    title="Filter by posture"
    aria-label="Filter by posture"
    aria-haspopup="dialog"
    aria-expanded={filterOpen}
    onclick={() => void (filterOpen ? closeFilter() : openFilter())}
  >
    <Filter size={15} strokeWidth={1.75} />
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Search map"
    aria-label="Search map"
    onclick={() => void openSearch()}
  >
    <Search size={15} strokeWidth={1.75} />
  </button>
{/if}

{#if momentsOpen && momentsPlacement}
  <BodyPortal>
    <div
      bind:this={momentsMenuEl}
      class="vault-dock-popover map-dock-moments-popover"
      role="listbox"
      aria-label="Moments"
      style:left="{momentsPlacement.left}px"
      style:top="{momentsPlacement.top}px"
      style:width="{momentsPlacement.width}px"
      style:max-height="{momentsPlacement.maxHeight}px"
      style:transform={momentsPlacement.transform}
      onclick={(event) => event.stopPropagation()}
    >
      <div class="map-dock-popover-search">
        <Search size={13} strokeWidth={1.75} class="shrink-0 text-surface-500" aria-hidden="true" />
        <input
          bind:this={momentsSearchEl}
          class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-surface-500 focus:outline-none focus:ring-0"
          type="search"
          placeholder="Search moments…"
          bind:value={momentsQuery}
        />
      </div>
      <div class="map-dock-popover-scroll">
        {#if contextThreads.loading && momentEntries.length === 0}
          <p class="workshop-faint px-2.5 py-3 text-[11px]">Loading moments…</p>
        {:else if momentEntries.length === 0}
          <p class="workshop-faint px-2.5 py-3 text-[11px] leading-relaxed">
            {momentsQuery.trim()
              ? "Nothing matches."
              : "No moments on the shelf yet."}
          </p>
        {:else}
          {#each momentEntries as entry (entry.id)}
            <button
              type="button"
              role="option"
              class="vault-dock-branch-option"
              onclick={() => pickMoment(entry.syncKey, entry.sessionId)}
            >
              <span class="vault-dock-branch-option__main min-w-0">
                <span class="vault-dock-branch-option__label line-clamp-2">{entry.title}</span>
                <span class="vault-dock-branch-option__meta truncate">{entry.subtitle}</span>
              </span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </BodyPortal>
{/if}

{#if filterOpen && filterPlacement}
  <BodyPortal>
    <div
      bind:this={filterMenuEl}
      class="vault-dock-popover map-dock-filter-popover"
      role="dialog"
      aria-label="Filter by posture"
      style:left="{filterPlacement.left}px"
      style:top="{filterPlacement.top}px"
      style:width="{filterPlacement.width}px"
      style:max-height="{filterPlacement.maxHeight}px"
      style:transform={filterPlacement.transform}
      onclick={(event) => event.stopPropagation()}
    >
      <div class="flex items-center justify-between gap-2 px-2.5 pb-1 pt-2">
        <p class="workshop-faint text-[11px] uppercase tracking-[0.08em]">AVEC</p>
        {#if filterActive}
          <button
            type="button"
            class="text-[11px] text-surface-400 transition hover:text-surface-100"
            onclick={() => contextShell.resetMapAvecMins()}
          >
            Reset
          </button>
        {/if}
      </div>
      <div class="space-y-3 px-2.5 pb-2.5 pt-1">
        {#each AVEC_DIMENSIONS as dim (dim.key)}
          <label class="block">
            <span class="mb-1 flex items-center justify-between text-[11px] text-surface-300">
              <span>{dim.label}</span>
              <span class="tabular-nums text-surface-500">{avecMins[dim.key].toFixed(2)}</span>
            </span>
            <input
              class="map-avec-dial"
              type="range"
              min="0"
              max="1"
              step="0.05"
              value={avecMins[dim.key]}
              oninput={(event) => {
                contextShell.setMapAvecMin(
                  dim.key,
                  Number((event.currentTarget as HTMLInputElement).value),
                );
              }}
            />
          </label>
        {/each}
        <p class="workshop-faint text-[10px] leading-relaxed">
          Show moments at or above each dial. 0 means no filter on that axis.
        </p>
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .map-dock-moments-popover,
  .map-dock-filter-popover {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.25rem 0;
  }

  .map-dock-popover-search {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin: 0.25rem 0.5rem 0.35rem;
    padding: 0.3rem 0.45rem;
    border-radius: 0.4rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-800) / 0.55);
  }

  .map-dock-popover-scroll {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
  }

  :global(.map-dock-moments-popover .vault-dock-branch-option__main) {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    align-items: flex-start;
  }

  :global(.map-dock-moments-popover .vault-dock-branch-option__meta) {
    font-size: 10px;
    color: rgb(var(--color-surface-500));
  }

  .map-avec-dial {
    width: 100%;
    accent-color: rgb(var(--color-secondary-400));
  }
</style>
