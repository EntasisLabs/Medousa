<script lang="ts">
  /**
   * You dock “+” menu — Add person / Teach / Add profile.
   */
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { placeToolbarPopover } from "$lib/utils/railPopover";
  import {
    dispatchProfilesAddPerson,
    dispatchProfilesAddProfile,
    dispatchProfilesFocusTeach,
  } from "$lib/utils/profilesChromeEvents";
  import { Brain, Plus, UserPlus, Users } from "@lucide/svelte";
  import { tick, type Component } from "svelte";

  type YouCreateId = "person" | "teach" | "profile";

  type YouCreateItem = {
    id: YouCreateId;
    label: string;
    icon: Component<{ size?: number; strokeWidth?: number }>;
  };

  interface Props {
    /** Prepare Profiles surface (mount panel + rail view) before dispatching. */
    onReady?: () => void | Promise<void>;
  }

  let { onReady }: Props = $props();

  const ITEMS: YouCreateItem[] = [
    { id: "person", label: "Add person", icon: UserPlus },
    { id: "teach", label: "Teach", icon: Brain },
    { id: "profile", label: "Add profile", icon: Users },
  ];

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  function close() {
    open = false;
  }

  function toggle(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    open = !open;
  }

  async function choose(id: YouCreateId) {
    close();
    await onReady?.();
    // Let ProfilesPanel / OverflowMenu mount listeners before the event lands.
    await tick();
    await tick();
    if (id === "person") dispatchProfilesAddPerson();
    else if (id === "teach") dispatchProfilesFocusTeach();
    else dispatchProfilesAddProfile();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  $effect(() => {
    if (!open || !triggerEl || !menuEl) return;
    let frame = 0;
    const place = () => {
      if (!triggerEl || !menuEl) return;
      placeToolbarPopover(triggerEl, menuEl, {
        prefer: "above",
        width: 11.5 * 16,
        gap: 6,
        pad: 8,
      });
      frame = window.requestAnimationFrame(() => {
        if (!triggerEl || !menuEl) return;
        placeToolbarPopover(triggerEl, menuEl, {
          prefer: "above",
          width: 11.5 * 16,
          gap: 6,
          pad: 8,
        });
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
    };
  });
</script>

<svelte:window onkeydown={onWindowKeydown} />

<button
  bind:this={triggerEl}
  type="button"
  class="vault-dock-icon-btn workshop-rail-row-action"
  class:workshop-rail-row-action-open={open}
  title="Add…"
  aria-label="Add…"
  aria-haspopup="menu"
  aria-expanded={open}
  onclick={toggle}
>
  <Plus size={14} strokeWidth={2} />
</button>

{#if open}
  <BodyPortal>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="you-create-menu-scrim" role="presentation" onclick={close}></div>
    <div
      bind:this={menuEl}
      class="you-create-menu workshop-rail-sheet"
      role="menu"
      aria-label="You create"
      onclick={(event) => event.stopPropagation()}
    >
      {#each ITEMS as item (item.id)}
        {@const Icon = item.icon}
        <button
          type="button"
          class="you-create-menu-item"
          role="menuitem"
          onclick={() => void choose(item.id)}
        >
          <span class="you-create-menu-item-icon" aria-hidden="true">
            <Icon size={15} strokeWidth={1.75} />
          </span>
          <span class="you-create-menu-item-label">{item.label}</span>
        </button>
      {/each}
    </div>
  </BodyPortal>
{/if}

<style>
  .you-create-menu-scrim {
    position: fixed;
    inset: 0;
    z-index: 140;
  }

  .you-create-menu {
    z-index: 145;
    display: flex;
    width: 11.5rem;
    flex-direction: column;
    gap: 0.1rem;
    padding: 0.3rem;
    overflow: hidden;
  }

  .you-create-menu-item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.55rem;
    border: 0;
    border-radius: 0.45rem;
    background: transparent;
    padding: 0.45rem 0.5rem;
    color: rgb(var(--color-surface-100));
    text-align: left;
    cursor: pointer;
  }

  .you-create-menu-item:hover {
    background: rgb(var(--color-surface-700) / 0.45);
  }

  .you-create-menu-item-icon {
    display: inline-flex;
    width: 1.15rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    color: rgb(var(--color-surface-300));
  }

  .you-create-menu-item-label {
    min-width: 0;
    flex: 1;
    font-size: 0.8125rem;
    font-weight: 500;
    line-height: 1.2;
  }

  :global(.workshop-rail-row-action-open) {
    opacity: 1;
    color: rgb(var(--color-surface-50));
  }
</style>
