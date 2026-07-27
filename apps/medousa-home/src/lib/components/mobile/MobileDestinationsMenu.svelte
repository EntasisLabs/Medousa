<script lang="ts">
  import {
    Activity,
    CalendarDays,
    Compass,
    Globe,
    Home,
    MessageCircle,
    NotebookText,
    Radio,
    Settings,
    Sparkles,
    UserRound,
    Users,
    X,
    Zap,
  } from "@lucide/svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import ProfileSwitcherCompact from "$lib/components/mobile/ProfileSwitcherCompact.svelte";
  import { registerMobileBackHandler, switchMobileTab } from "$lib/mobileNavigation";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { haptic } from "$lib/haptics";
  import {
    mobileDestinationSections,
    type MobileDestinationItem,
  } from "$lib/utils/mobileDestinations";
  import {
    popBrowserPopoverOverlay,
    pushBrowserPopoverOverlay,
  } from "$lib/utils/browserPopoverOverlay";
  import type { Component } from "svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
    onToggleActivity?: () => void;
    /** Align sheet under the menu trigger (home puts menu on the right). */
    align?: "start" | "end";
  }

  let { open, onClose, onToggleActivity, align = "start" }: Props = $props();

  const icons: Record<string, Component> = {
    "tab-home": Home,
    "tab-chat": MessageCircle,
    "tab-notes": NotebookText,
    "tab-web": Globe,
    "more-calendar": CalendarDays,
    "more-profiles": UserRound,
    "more-map": Compass,
    "more-workshop": Sparkles,
    "more-automations": Zap,
    "more-messaging": Radio,
    "more-peers": Users,
    "more-settings": Settings,
    "more-runtime": Activity,
  };

  const sections = $derived(mobileDestinationSections());
  const customViews = $derived(
    environment.navSurfaces().filter((surface) => surface.kind === "custom"),
  );

  function pick(item: MobileDestinationItem) {
    haptic("light");
    onClose();
    if (item.kind === "tab" && item.tab) {
      switchMobileTab(item.tab);
      return;
    }
    if (item.kind === "more" && item.more) {
      layout.openMore(item.more);
    }
  }

  function pickCustom(surfaceId: string) {
    haptic("light");
    onClose();
    layout.openCustomSurface(surfaceId);
  }

  $effect(() => {
    if (!open) return;
    return registerMobileBackHandler(() => {
      onClose();
      return true;
    });
  });

  /** Native WKWebView paints above DOM — hide the embed while the menu is open. */
  $effect(() => {
    if (!open) return;
    void pushBrowserPopoverOverlay();
    return () => {
      void popBrowserPopoverOverlay();
    };
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="mobile-dest-menu-backdrop mobile-dest-menu-backdrop--{align}"
    role="presentation"
    onclick={onClose}
    onkeydown={(e) => {
      if (e.key === "Escape") onClose();
    }}
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="mobile-dest-menu"
      role="dialog"
      aria-modal="true"
      aria-label="Destinations"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <header class="mobile-dest-menu-head">
        <h2 class="mobile-dest-menu-title">Menu</h2>
        <button type="button" class="mobile-icon-btn" aria-label="Close" onclick={onClose}>
          <X size={18} strokeWidth={1.75} />
        </button>
      </header>

      <div class="mobile-dest-menu-switchers">
        <WorkshopSwitcherCompact />
        <ProfileSwitcherCompact />
        {#if onToggleActivity}
          <button
            type="button"
            class="mobile-dest-menu-activity"
            onclick={() => {
              onClose();
              onToggleActivity();
            }}
          >
            Activity
          </button>
        {/if}
      </div>

      <div class="mobile-dest-menu-scroll">
        {#each sections as section (section.title)}
          <p class="mobile-dest-menu-section">{section.title}</p>
          <ul class="mobile-dest-menu-list">
            {#each section.items as item (item.id)}
              {@const Icon = icons[item.id] ?? Sparkles}
              <li>
                <button type="button" class="mobile-dest-menu-row" onclick={() => pick(item)}>
                  <Icon size={18} strokeWidth={1.75} class="mobile-dest-menu-icon" />
                  <span class="min-w-0 flex-1 text-left">
                    <span class="block text-[15px] font-medium text-surface-50">{item.label}</span>
                    {#if item.hint}
                      <span class="block truncate text-[11px] text-surface-500">{item.hint}</span>
                    {/if}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/each}

        {#if customViews.length > 0}
          <p class="mobile-dest-menu-section">My views</p>
          <ul class="mobile-dest-menu-list">
            {#each customViews as view (view.id)}
              <li>
                <button
                  type="button"
                  class="mobile-dest-menu-row"
                  onclick={() => pickCustom(view.id)}
                >
                  <Sparkles size={18} strokeWidth={1.75} class="mobile-dest-menu-icon" />
                  <span class="text-[15px] font-medium text-surface-50">{view.label}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  </div>
{/if}
