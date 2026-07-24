<script lang="ts">
  import NavShell from "$lib/components/layout/NavShell.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import {
    layout,
    SHELL_SIDEBAR_WIDTH_MAX,
    SHELL_SIDEBAR_WIDTH_MIN,
  } from "$lib/stores/layout.svelte";
  import type { DaemonHealth } from "$lib/daemon";

  interface Props {
    active: string;
    onSelect: (surface: string) => void;
    onOpenChat: () => void;
    health?: DaemonHealth | null;
    chatActivity?: number;
    workActivity?: number;
    peersActivity?: number;
    activeProfileLabel?: string;
  }

  let {
    active,
    onSelect,
    onOpenChat,
    health = null,
    chatActivity = 0,
    workActivity = 0,
    peersActivity = 0,
    activeProfileLabel = "Personal",
  }: Props = $props();

  const open = $derived(layout.shellSidebarExpanded);
  const width = $derived(layout.shellSidebarWidth);
  const slotWidth = $derived(open ? width : 0);

  let resizing = $state(false);
</script>

<!--
  Keep NavShell mounted when collapsed. Summon (shake / ⌘⇧.) registers on NavShell
  mount — unmounting the rail made “toolbar for active view” a no-op exactly when
  you need it (deep in content, rail tucked away).
-->
<div
  class="master-rail-slot"
  class:master-rail-slot-open={open}
  class:master-rail-slot-resizing={resizing}
  style="width: {slotWidth}px"
  data-debug-label="master-rail-slot"
  aria-hidden={!open}
>
  <div class="master-rail-inner" style="width: {width}px">
    <SplitPane
      {width}
      side="left"
      min={SHELL_SIDEBAR_WIDTH_MIN}
      max={SHELL_SIDEBAR_WIDTH_MAX}
      onResize={(next) => {
        resizing = true;
        layout.setShellSidebarWidth(next);
      }}
      onResizeEnd={() => {
        resizing = false;
      }}
    >
      <NavShell
        {active}
        {onSelect}
        {onOpenChat}
        {health}
        {chatActivity}
        {workActivity}
        {peersActivity}
        {activeProfileLabel}
      />
    </SplitPane>
  </div>
</div>

<style>
  .master-rail-slot {
    position: relative;
    z-index: 2;
    height: 100%;
    flex-shrink: 0;
    overflow: hidden;
    transition: width 220ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  .master-rail-slot-resizing {
    transition: none;
  }

  .master-rail-inner {
    height: 100%;
  }

  @media (prefers-reduced-motion: reduce) {
    .master-rail-slot {
      transition: none;
    }
  }
</style>
