<script lang="ts">
  import { placeRailPopover } from "$lib/utils/railPopover";
  import type { Snippet } from "svelte";
  import { tick } from "svelte";

  interface Props {
    open: boolean;
    title: string;
    triggerEl: HTMLElement | null;
    onClose: () => void;
    children: Snippet;
  }

  let { open, title, triggerEl, onClose, children }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    if (!open || !triggerEl || !menuEl) return;
    let frame = 0;
    const place = () => {
      if (!triggerEl || !menuEl) return;
      placeRailPopover(triggerEl, menuEl, { gap: 10, pad: 10 });
      frame = window.requestAnimationFrame(() => {
        if (triggerEl && menuEl) placeRailPopover(triggerEl, menuEl, { gap: 10, pad: 10 });
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
    };
  });

  // Attach outside the opening click so the same gesture doesn't instantly dismiss.
  $effect(() => {
    if (!open) return;
    let ready = false;
    const arm = window.setTimeout(() => {
      ready = true;
    }, 0);

    const onClick = (event: MouseEvent) => {
      if (!ready) return;
      const target = event.target as Node | null;
      if (!target) return;
      if (menuEl?.contains(target) || triggerEl?.contains(target)) return;
      onClose();
    };

    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };

    window.addEventListener("click", onClick);
    window.addEventListener("keydown", onKeydown);
    return () => {
      window.clearTimeout(arm);
      window.removeEventListener("click", onClick);
      window.removeEventListener("keydown", onKeydown);
    };
  });
</script>

{#if open}
  <div
    bind:this={menuEl}
    class="nav-rail-view-popover"
    role="dialog"
    aria-label={title}
    data-debug-label="nav-rail-view-popover"
    onclick={(event) => event.stopPropagation()}
  >
    <header class="nav-rail-view-popover-head">
      <p class="nav-rail-view-popover-title">{title}</p>
    </header>
    <div class="nav-rail-view-popover-body">
      {@render children()}
    </div>
  </div>
{/if}

<style>
  .nav-rail-view-popover {
    position: fixed;
    z-index: 80;
    display: flex;
    width: min(22rem, calc(100vw - 2rem));
    height: min(32rem, calc(100vh - 2rem));
    flex-direction: column;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-600) / 0.32);
    border-radius: 0.75rem;
    background: rgb(var(--color-surface-900) / 0.98);
    box-shadow:
      0 14px 36px rgb(0 0 0 / 0.36),
      0 0 0 1px rgb(255 255 255 / 0.03);
    animation: nav-rail-view-popover-in 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes nav-rail-view-popover-in {
    from {
      opacity: 0;
      transform: translateX(-0.35rem) scale(0.985);
    }
    to {
      opacity: 1;
      transform: translateX(0) scale(1);
    }
  }

  .nav-rail-view-popover-head {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    border-bottom: 1px solid rgb(var(--color-surface-700) / 0.35);
    padding: 0.55rem 0.75rem;
  }

  .nav-rail-view-popover-title {
    margin: 0;
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-100));
  }

  .nav-rail-view-popover-body {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    overflow: hidden;
  }
</style>
