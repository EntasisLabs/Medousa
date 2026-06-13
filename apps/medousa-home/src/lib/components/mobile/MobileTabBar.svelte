<script lang="ts">
  import { Activity, LayoutGrid, MessageCircle, User } from "@lucide/svelte";
  import { haptic } from "$lib/haptics";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { MOBILE_TABS, type MobileTab } from "$lib/types/mobile";
  import type { Component } from "svelte";

  const icons: Record<MobileTab, Component> = {
    pulse: Activity,
    work: LayoutGrid,
    chat: MessageCircle,
    you: User,
  };

  const iconProps = { size: 20, strokeWidth: 2 };
</script>

<nav class="mobile-tab-bar-inner" aria-label="Primary">
  {#each MOBILE_TABS as tab (tab.id)}
    {@const Icon = icons[tab.id]}
    <button
      type="button"
      class="mobile-tab-btn {layout.mobileTab === tab.id ? 'mobile-tab-btn-active' : ''}"
      aria-current={layout.mobileTab === tab.id ? "page" : undefined}
      onclick={() => {
        haptic("light");
        layout.setActivitySheetOpen(false);
        layout.setAskSheetOpen(false);
        if (tab.id !== "chat") {
          layout.setSessionDrawerOpen(false);
          layout.setIdentityDrawerOpen(false);
        }
        layout.setMobileTab(tab.id);
        if (tab.id === "you") {
          layout.backToYouHub();
        } else if (tab.id === "chat") {
          layout.backToYouHub();
          void chat.refreshSessions();
          void chat.ensureSessionHydrated();
        }
      }}
    >
      <Icon {...iconProps} />
      <span>{tab.label}</span>
    </button>
  {/each}
</nav>
