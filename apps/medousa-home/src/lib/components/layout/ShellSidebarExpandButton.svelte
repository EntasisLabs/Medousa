<script lang="ts">
  import { PanelLeftOpen } from "@lucide/svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { usesUnifiedTitlebar } from "$lib/platform";

  interface Props {
    /** Optional label override for title/aria. */
    label?: string;
  }

  let { label = "Show rail" }: Props = $props();

  /** AppTitlebar owns rail toggle on desktop Tauri; keep in-pane expand for web/mobile. */
  const show = $derived(
    !usesUnifiedTitlebar() && !layout.isMobile && !layout.shellSidebarExpanded,
  );

  function expand() {
    layout.openShellSidebarView(layout.desktopSurface);
    void environment.patchShellChromeDesktop({ navStyle: "rail" }).catch(() => {});
  }
</script>

{#if show}
  <button
    type="button"
    class="shell-sidebar-expand-btn"
    title={label}
    aria-label={label}
    onclick={expand}
  >
    <PanelLeftOpen size={15} strokeWidth={1.75} />
  </button>
{/if}

<style>
  .shell-sidebar-expand-btn {
    display: inline-flex;
    width: 1.75rem;
    height: 1.75rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.375rem;
    color: rgb(var(--color-surface-400));
    animation: shell-sidebar-expand-in 180ms cubic-bezier(0.22, 1, 0.36, 1);
    transition:
      color 120ms ease,
      background 120ms ease,
      transform 120ms ease;
  }

  .shell-sidebar-expand-btn:hover {
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
    color: rgb(var(--color-surface-200));
  }

  .shell-sidebar-expand-btn:active {
    transform: scale(0.94);
  }

  @keyframes shell-sidebar-expand-in {
    from {
      opacity: 0;
      transform: scale(0.88);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .shell-sidebar-expand-btn {
      animation: none;
    }
  }
</style>
