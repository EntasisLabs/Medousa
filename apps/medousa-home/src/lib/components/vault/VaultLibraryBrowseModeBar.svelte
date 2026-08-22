<script lang="ts">
  import { Check, Clock, FolderTree, Shapes, Tags } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import {
    vault,
    type LibraryBrowseMode,
  } from "$lib/stores/vault.svelte";
  import {
    placeDockPopover,
    type DockPopoverPlacement,
  } from "$lib/utils/dockPopoverPlace";
  import type { Component } from "svelte";

  interface Props {
    /** Drop horizontal padding when nested in an already-padded parent. */
    flush?: boolean;
    /** Text chips — no pill chrome. Recent first for a short path. */
    quiet?: boolean;
    /** Icon buttons with tooltips (dock / chrome). */
    icons?: boolean;
    /** Rail selector: Recent + Folders + Tags (Kind stays on full library chrome). */
    rail?: boolean;
  }

  let { flush = false, quiet = false, icons = false, rail = false }: Props = $props();
  let menuOpen = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let placement = $state<DockPopoverPlacement | null>(null);

  const allModes: {
    id: LibraryBrowseMode;
    label: string;
    Icon: Component;
  }[] = [
    { id: "recent", label: "Recent", Icon: Clock },
    { id: "folders", label: "Folders", Icon: FolderTree },
    { id: "tags", label: "Tags", Icon: Tags },
    { id: "kind", label: "Kind", Icon: Shapes },
  ];

  const modes = $derived(
    rail
      ? allModes.filter(
          (mode) =>
            mode.id === "recent" ||
            mode.id === "folders" ||
            mode.id === "tags",
        )
      : allModes,
  );
  const currentMode = $derived(
    modes.find((mode) => mode.id === vault.libraryBrowseMode) ?? modes[0],
  );

  function selectMode(mode: LibraryBrowseMode) {
    if (vault.searchQuery.trim()) void vault.runSearch("");
    vault.setLibraryBrowseMode(mode);
    closeMenu();
  }

  function placeMenu() {
    if (!triggerEl) return;
    placement = placeDockPopover(triggerEl, {
      preferUp: false,
      width: 168,
    });
  }

  function closeMenu() {
    menuOpen = false;
    placement = null;
  }

  function toggleMenu(event: MouseEvent) {
    event.stopPropagation();
    if (menuOpen) {
      closeMenu();
      return;
    }
    menuOpen = true;
    requestAnimationFrame(placeMenu);
  }

  function onWindowPointerDown(event: PointerEvent) {
    if (!menuOpen) return;
    const target = event.target as Node;
    if (triggerEl?.contains(target) || menuEl?.contains(target)) return;
    closeMenu();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (menuOpen && event.key === "Escape") {
      event.preventDefault();
      closeMenu();
    }
  }

  $effect(() => {
    if (!menuOpen) return;
    window.addEventListener("pointerdown", onWindowPointerDown, true);
    window.addEventListener("keydown", onWindowKeydown);
    window.addEventListener("resize", placeMenu);
    window.addEventListener("scroll", placeMenu, true);
    return () => {
      window.removeEventListener("pointerdown", onWindowPointerDown, true);
      window.removeEventListener("keydown", onWindowKeydown);
      window.removeEventListener("resize", placeMenu);
      window.removeEventListener("scroll", placeMenu, true);
    };
  });
</script>

{#if rail && currentMode}
  {@const CurrentIcon = currentMode.Icon}
  <div class="relative min-w-0 shrink">
    <button
      bind:this={triggerEl}
      type="button"
      class="vault-dock-branch vault-browse-mode-select"
      class:vault-dock-branch--active={menuOpen}
      aria-haspopup="listbox"
      aria-expanded={menuOpen}
      aria-label="Browse by {currentMode.label}"
      title="Browse by {currentMode.label}"
      onclick={toggleMenu}
    >
      <CurrentIcon size={13} strokeWidth={1.75} class="vault-dock-branch__icon" />
      <span class="vault-dock-branch__label">{currentMode.label}</span>
    </button>
  </div>
  {#if menuOpen && placement}
    <BodyPortal>
      <div
        bind:this={menuEl}
        class="vault-dock-popover"
        role="listbox"
        tabindex="-1"
        aria-label="Browse notes by"
        style:left="{placement.left}px"
        style:top="{placement.top}px"
        style:width="{placement.width}px"
        style:max-height="{placement.maxHeight}px"
        style:transform={placement.transform}
        onclick={(event) => event.stopPropagation()}
        onkeydown={(event) => event.stopPropagation()}
      >
        {#each modes as mode (mode.id)}
          {@const ModeIcon = mode.Icon}
          <button
            type="button"
            role="option"
            aria-selected={vault.libraryBrowseMode === mode.id}
            class="vault-dock-branch-option"
            class:vault-dock-branch-option--selected={vault.libraryBrowseMode === mode.id}
            onclick={() => selectMode(mode.id)}
          >
            <span class="vault-dock-branch-option__main">
              <ModeIcon size={13} strokeWidth={1.75} class="vault-dock-branch-option__icon" />
              <span class="vault-dock-branch-option__label">{mode.label}</span>
            </span>
            {#if vault.libraryBrowseMode === mode.id}
              <Check size={13} strokeWidth={2} class="vault-dock-branch-option__check" />
            {/if}
          </button>
        {/each}
      </div>
    </BodyPortal>
  {/if}
{:else if icons}
  <div
    class="vault-browse-mode-icons {flush ? 'vault-browse-mode-icons--flush' : ''}"
    role="tablist"
    aria-label="Library browse mode"
  >
    {#each modes as mode (mode.id)}
      {@const Icon = mode.Icon}
      <button
        type="button"
        role="tab"
        aria-selected={vault.libraryBrowseMode === mode.id && !vault.searchQuery.trim()}
        class="vault-dock-icon-btn {vault.libraryBrowseMode === mode.id &&
        !vault.searchQuery.trim()
          ? 'vault-dock-icon-btn-active'
          : ''}"
        title={mode.label}
        aria-label={mode.label}
        onclick={() => selectMode(mode.id)}
      >
        <Icon size={15} strokeWidth={1.75} />
      </button>
    {/each}
  </div>
{:else}
  <div
    class="vault-browse-mode-bar {flush ? 'vault-browse-mode-bar--flush' : ''} {quiet
      ? 'vault-browse-mode-bar--quiet'
      : ''}"
    role="tablist"
    aria-label="Library browse mode"
  >
    {#each modes as mode (mode.id)}
      <button
        type="button"
        role="tab"
        aria-selected={vault.libraryBrowseMode === mode.id}
        class="vault-browse-mode-btn {quiet ? 'vault-browse-mode-btn--quiet' : ''} {vault.libraryBrowseMode ===
        mode.id
          ? quiet
            ? 'vault-browse-mode-btn-quiet-active'
            : 'vault-browse-mode-btn-active'
          : ''}"
        onclick={() => selectMode(mode.id)}
      >
        {mode.label}
      </button>
    {/each}
  </div>
{/if}
