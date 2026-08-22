<script lang="ts">
  import { Settings } from "@lucide/svelte";

  interface Props {
    popover?: boolean;
  }

  let { popover = false }: Props = $props();
</script>

<div
  class="settings-rail-title"
  class:nav-rail-popover-toolbar-label={popover}
>
  <Settings size={13} strokeWidth={1.75} aria-hidden="true" />
  <span>Settings</span>
</div>

<style>
  .settings-rail-title {
    display: inline-flex;
    min-width: 0;
    align-items: center;
    gap: 0.4rem;
    padding-inline: 0.4rem;
    color: rgb(var(--color-surface-200));
    font-size: 0.78rem;
    font-weight: 550;
    letter-spacing: -0.012em;
  }

  .settings-rail-title :global(svg) {
    flex-shrink: 0;
    color: rgb(var(--theme-text-tertiary));
    opacity: 0.9;
  }
</style>
