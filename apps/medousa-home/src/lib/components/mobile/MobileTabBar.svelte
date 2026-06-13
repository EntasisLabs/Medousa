<script lang="ts">
  import { Activity, LayoutGrid, MessageCircle, User } from "@lucide/svelte";
  import { switchMobileTab } from "$lib/mobileNavigation";
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
      onclick={() => switchMobileTab(tab.id)}
    >
      <Icon {...iconProps} />
      <span>{tab.label}</span>
    </button>
  {/each}
</nav>
