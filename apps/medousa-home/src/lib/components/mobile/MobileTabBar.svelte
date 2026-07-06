<script lang="ts">
  import { Activity, BookOpen, Globe, MessageCircle, MoreHorizontal } from "@lucide/svelte";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { MOBILE_TABS, type MobileTab } from "$lib/types/mobile";
  import { visibleMobileTabs } from "$lib/utils/mobileEnvironmentChrome";
  import type { Component } from "svelte";

  const icons: Record<MobileTab, Component> = {
    home: Activity,
    chat: MessageCircle,
    notes: BookOpen,
    web: Globe,
    more: MoreHorizontal,
  };

  const iconProps = { size: 20, strokeWidth: 2 };

  const tabs = $derived(
    MOBILE_TABS.filter((tab) => visibleMobileTabs(environment.spec).includes(tab.id)),
  );
</script>

<nav class="mobile-tab-bar-inner" aria-label="Primary">
  {#each tabs as tab (tab.id)}
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
