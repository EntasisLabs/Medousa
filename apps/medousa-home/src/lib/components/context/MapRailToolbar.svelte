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
      width: 300,
      maxHeight: 380,
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
    <Search size={14} strokeWidth={1.75} class="shrink-0 text-content-quiet" aria-hidden="true" />
    <input
      bind:this={searchInputEl}
      class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-content-quiet focus:outline-none focus:ring-0"
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
      tabindex="-1"
      aria-label="Moments"
      style:left="{momentsPlacement.left}px"
      style:top="{momentsPlacement.top}px"
      style:width="{momentsPlacement.width}px"
      style:max-height="{momentsPlacement.maxHeight}px"
      style:transform={momentsPlacement.transform}
      onclick={(event) => event.stopPropagation()}
      onkeydown={(event) => event.stopPropagation()}
    >
      <div class="map-dock-popover-search">
        <Search size={13} strokeWidth={1.75} class="shrink-0 text-content-quiet" aria-hidden="true" />
        <input
          bind:this={momentsSearchEl}
          class="map-dock-moments-input"
          type="search"
          placeholder="Find a moment…"
          bind:value={momentsQuery}
        />
      </div>
      <div class="map-dock-popover-scroll">
        {#if contextThreads.loading && momentEntries.length === 0}
          <p class="map-dock-moments-empty">Loading…</p>
        {:else if momentEntries.length === 0}
          <p class="map-dock-moments-empty">
            {momentsQuery.trim() ? "Nothing matches." : "No moments yet."}
          </p>
        {:else}
          {#each momentEntries as entry (entry.id)}
            <button
              type="button"
              role="option"
              aria-selected={false}
              class="map-dock-moment-row"
              onclick={() => pickMoment(entry.syncKey, entry.sessionId)}
            >
              <span class="map-dock-moment-row-title">{entry.title}</span>
              <span class="map-dock-moment-row-meta">{entry.subtitle}</span>
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
      tabindex="-1"
      aria-label="Filter by posture"
      style:left="{filterPlacement.left}px"
      style:top="{filterPlacement.top}px"
      style:width="{filterPlacement.width}px"
      style:max-height="{filterPlacement.maxHeight}px"
      style:transform={filterPlacement.transform}
      onclick={(event) => event.stopPropagation()}
      onkeydown={(event) => event.stopPropagation()}
    >
      <div class="flex items-center justify-between gap-2 px-2.5 pb-1 pt-2">
        <p class="workshop-faint text-[11px] uppercase tracking-[0.08em]">AVEC</p>
        {#if filterActive}
          <button
            type="button"
            class="text-[11px] text-content-tertiary transition hover:text-surface-100"
            onclick={() => contextShell.resetMapAvecMins()}
          >
            Reset
          </button>
        {/if}
      </div>
      <div class="space-y-3 px-2.5 pb-2.5 pt-1">
        {#each AVEC_DIMENSIONS as dim (dim.key)}
          <label class="block">
            <span class="mb-1 flex items-center justify-between text-[11px] text-content-secondary">
              <span>{dim.label}</span>
              <span class="tabular-nums text-content-quiet">{avecMins[dim.key].toFixed(2)}</span>
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
    padding: 0.35rem 0 0.4rem;
  }

  .map-dock-popover-search {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin: 0.15rem 0.7rem 0.45rem;
    padding: 0.2rem 0 0.45rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.28);
    background: transparent;
  }

  .map-dock-moments-input {
    min-width: 0;
    flex: 1;
    border: 0;
    background: transparent;
    font-size: 12px;
    color: rgb(var(--color-surface-100));
    outline: none;
  }

  .map-dock-moments-input::placeholder {
    color: rgb(var(--theme-text-quiet));
  }

  .map-dock-popover-scroll {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    padding: 0.1rem 0.35rem 0.15rem;
  }

  .map-dock-moments-empty {
    margin: 0;
    padding: 1rem 0.85rem;
    font-size: 11px;
    line-height: 1.45;
    color: rgb(var(--theme-text-quiet));
  }

  .map-dock-moment-row {
    display: flex;
    width: 100%;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.2rem;
    border: 0;
    border-radius: 0.55rem;
    background: transparent;
    padding: 0.55rem 0.65rem;
    text-align: left;
    cursor: pointer;
  }

  .map-dock-moment-row:hover {
    background: rgb(var(--color-surface-800) / 0.72);
  }

  .map-dock-moment-row-title {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    max-width: 100%;
    font-size: 0.8125rem;
    font-weight: 520;
    letter-spacing: -0.015em;
    line-height: 1.3;
    color: rgb(var(--color-surface-50));
  }

  .map-dock-moment-row-meta {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10px;
    letter-spacing: 0.01em;
    color: rgb(var(--theme-text-quiet));
  }

  .map-avec-dial {
    width: 100%;
    accent-color: rgb(var(--color-secondary-400));
  }
</style>
