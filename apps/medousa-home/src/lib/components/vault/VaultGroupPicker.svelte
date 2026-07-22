<script lang="ts">
  import { Check, Layers } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { selectableGroupSpaces } from "$lib/config/vaultSpaces";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultFolderIcons } from "$lib/stores/vaultFolderIcons.svelte";
  import {
    placeDockPopover,
    type DockPopoverPlacement,
  } from "$lib/utils/dockPopoverPlace";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";

  interface Props {
    /** Prefer opening the menu above the trigger (bottom dock). */
    dropUp?: boolean;
  }

  let { dropUp = true }: Props = $props();

  let menuOpen = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let placement = $state<DockPopoverPlacement | null>(null);

  const folderIconMap = $derived(vaultFolderIcons.icons);
  const visibleSpaces = $derived(selectableGroupSpaces(vault.showSystemNotes));
  const spaceCounts = $derived(vault.spaceCountsMap);

  const activeSpace = $derived.by(() => {
    if (!vault.activeSpaceFilter) return null;
    return visibleSpaces.find((space) => space.id === vault.activeSpaceFilter) ?? null;
  });

  const activeLabel = $derived(activeSpace?.label ?? "All notes");
  const hasExtras = $derived(vault.showAgentReviewFilter || vault.showSystemNotes);

  function place() {
    if (!triggerEl) return;
    placement = placeDockPopover(triggerEl, { preferUp: dropUp, width: 216 });
  }

  function close() {
    menuOpen = false;
    placement = null;
  }

  function open() {
    menuOpen = true;
    requestAnimationFrame(place);
  }

  function toggle(event: MouseEvent) {
    event.stopPropagation();
    if (menuOpen) close();
    else open();
  }

  function selectSpace(spaceId: string | null) {
    vault.setActiveSpaceFilter(spaceId);
    close();
  }

  function clearGroup() {
    vault.setActiveSpaceFilter(null);
    vault.setShowAgentReviewFilter(false);
    vault.setShowSystemNotes(false);
    close();
  }

  function onWindowPointerDown(event: PointerEvent) {
    if (!menuOpen) return;
    const target = event.target as Node;
    if (triggerEl?.contains(target) || menuEl?.contains(target)) return;
    close();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (!menuOpen) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  function onWindowReposition() {
    if (menuOpen) place();
  }

  $effect(() => {
    if (!menuOpen) return;
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

<div class="relative min-w-0 shrink">
  <button
    bind:this={triggerEl}
    type="button"
    class="vault-dock-branch"
    class:vault-dock-branch--active={Boolean(vault.activeSpaceFilter) || hasExtras}
    aria-haspopup="listbox"
    aria-expanded={menuOpen}
    aria-label="Group: {activeLabel}"
    title="Switch group"
    onclick={toggle}
  >
    {#if activeSpace}
      {@const _ = folderIconMap}
      {@const ActiveIcon = iconForSpace(activeSpace.id)}
      <ActiveIcon size={13} strokeWidth={1.75} class="vault-dock-branch__icon" />
    {:else}
      <Layers size={13} strokeWidth={1.75} class="vault-dock-branch__icon" />
    {/if}
    <span class="vault-dock-branch__label">{activeLabel}</span>
  </button>
</div>

{#if menuOpen && placement}
  <BodyPortal>
    <div
      bind:this={menuEl}
      class="vault-dock-popover"
      role="listbox"
      aria-label="Groups"
      style:left="{placement.left}px"
      style:top="{placement.top}px"
      style:width="{placement.width}px"
      style:max-height="{placement.maxHeight}px"
      style:transform={placement.transform}
      onclick={(event) => event.stopPropagation()}
    >
      <button
        type="button"
        role="option"
        aria-selected={vault.activeSpaceFilter === null}
        class="vault-dock-branch-option"
        class:vault-dock-branch-option--selected={vault.activeSpaceFilter === null}
        onclick={() => selectSpace(null)}
      >
        <span class="vault-dock-branch-option__main">
          <Layers size={13} strokeWidth={1.75} class="vault-dock-branch-option__icon" />
          <span class="vault-dock-branch-option__label">All notes</span>
        </span>
        {#if vault.activeSpaceFilter === null}
          <Check size={13} strokeWidth={2} class="vault-dock-branch-option__check" />
        {/if}
      </button>

      {#each visibleSpaces as space (space.id)}
        {@const _ = folderIconMap}
        {@const Icon = iconForSpace(space.id)}
        {@const count = spaceCounts.get(space.id) ?? 0}
        <button
          type="button"
          role="option"
          aria-selected={vault.activeSpaceFilter === space.id}
          class="vault-dock-branch-option"
          class:vault-dock-branch-option--selected={vault.activeSpaceFilter === space.id}
          onclick={() => selectSpace(space.id)}
        >
          <span class="vault-dock-branch-option__main">
            <Icon size={13} strokeWidth={1.75} class="vault-dock-branch-option__icon" />
            <span class="vault-dock-branch-option__label">{space.label}</span>
            {#if count > 0}
              <span class="vault-dock-branch-option__meta">{count}</span>
            {/if}
          </span>
          {#if vault.activeSpaceFilter === space.id}
            <Check size={13} strokeWidth={2} class="vault-dock-branch-option__check" />
          {/if}
        </button>
      {/each}

      <div class="vault-dock-popover__sep"></div>

      <button
        type="button"
        role="option"
        aria-selected={vault.showAgentReviewFilter}
        class="vault-dock-branch-option vault-dock-branch-option--soft"
        class:vault-dock-branch-option--selected={vault.showAgentReviewFilter}
        onclick={() => vault.setShowAgentReviewFilter(!vault.showAgentReviewFilter)}
      >
        <span class="vault-dock-branch-option__main">
          <span class="vault-dock-branch-option__label">Agent review</span>
        </span>
        {#if vault.showAgentReviewFilter}
          <Check size={13} strokeWidth={2} class="vault-dock-branch-option__check" />
        {/if}
      </button>
      <button
        type="button"
        role="option"
        aria-selected={vault.showSystemNotes}
        class="vault-dock-branch-option vault-dock-branch-option--soft"
        class:vault-dock-branch-option--selected={vault.showSystemNotes}
        onclick={() => vault.setShowSystemNotes(!vault.showSystemNotes)}
      >
        <span class="vault-dock-branch-option__main">
          <span class="vault-dock-branch-option__label">Developer notes</span>
        </span>
        {#if vault.showSystemNotes}
          <Check size={13} strokeWidth={2} class="vault-dock-branch-option__check" />
        {/if}
      </button>

      {#if vault.activeSpaceFilter !== null || hasExtras}
        <div class="vault-dock-popover__sep"></div>
        <button
          type="button"
          role="option"
          class="vault-dock-branch-option vault-dock-branch-option--soft"
          onclick={clearGroup}
        >
          <span class="vault-dock-branch-option__main">
            <span class="vault-dock-branch-option__label">Clear group</span>
          </span>
        </button>
      {/if}
    </div>
  </BodyPortal>
{/if}
