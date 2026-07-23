<script lang="ts">
  import { ChevronRight, Expand } from "@lucide/svelte";
  import {
    placeRailPopover,
    railPopoverOpenHeightCap,
    resolveRailPopoverExpand,
    type RailPopoverCursor,
    type RailPopoverExpand,
  } from "$lib/utils/railPopover";
  import { popLmeDockHost, pushLmeDockHost } from "$lib/utils/lmeDockHost";
  import type { Snippet } from "svelte";
  import { tick } from "svelte";

  type PopoverPhase = "seed" | "toolbar" | "open";

  interface Props {
    open: boolean;
    title: string;
    /** Reset expand state when this changes (e.g. surface/mode id). */
    targetKey?: string;
    triggerEl: HTMLElement | null;
    /**
     * Click point — float the toolbar next to the mouse (selection-bubble style)
     * instead of docking beside the rail.
     */
    cursorAnchor?: RailPopoverCursor | null;
    onClose: () => void;
    /** Dock this list into the master side rail (view mode). */
    onDockToRail?: () => void;
    /** Left-side action controls (New, Search, dock buttons, etc.). */
    toolbar?: Snippet;
    /** Host LME explorer docks in the toolbar strip while open. */
    dockHost?: boolean;
    children: Snippet;
  }

  let {
    open,
    title,
    targetKey = "",
    triggerEl,
    cursorAnchor = null,
    onClose,
    onDockToRail,
    toolbar,
    dockHost = false,
    children,
  }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  let dockSlotEl = $state<HTMLElement | null>(null);
  let phase = $state<PopoverPhase>("seed");
  let lastTargetKey = $state("");
  let sequenceGen = 0;
  /** Freeze trigger-adjacent edge while height/width animate. */
  let anchorLocked = $state(false);
  /** Persist for the popover session so expand/collapse doesn't flip mid-gesture. */
  let expandDir = $state<RailPopoverExpand>("down");
  let openHeightPx = $state(32 * 16);

  const listOpen = $derived(phase === "open");
  const chromeExpanded = $derived(phase === "toolbar" || phase === "open");
  const expandUp = $derived(expandDir === "up");

  /** Match CSS transition durations in this component. */
  const WIDTH_MS = 320;
  const HEIGHT_MS = 360;
  const CHROME_MS = 300;
  const PLACE_PAD = 10;
  const OPEN_MAX_PX = 32 * 16;

  function prefersReducedMotion() {
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  }

  function sleep(ms: number) {
    return new Promise<void>((resolve) => {
      window.setTimeout(resolve, ms);
    });
  }

  function syncGeometryFromTrigger() {
    if (!triggerEl) return;
    expandDir = resolveRailPopoverExpand(triggerEl, {
      pad: PLACE_PAD,
      cursor: cursorAnchor,
    });
    openHeightPx = railPopoverOpenHeightCap(triggerEl, expandDir, {
      pad: PLACE_PAD,
      maxHeight: OPEN_MAX_PX,
      cursor: cursorAnchor,
    });
  }

  function placeMenu(opts?: { lockEdge?: boolean }) {
    if (!triggerEl || !menuEl) return;
    const lock = opts?.lockEdge ?? anchorLocked;
    placeRailPopover(triggerEl, menuEl, {
      gap: 10,
      pad: PLACE_PAD,
      alignY: "start",
      expand: expandDir,
      openHeight: openHeightPx,
      cursor: cursorAnchor,
      lockEdge: lock ? (expandDir === "up" ? "bottom" : "top") : undefined,
    });
    menuEl.style.setProperty("--nav-rail-popover-open-height", `${openHeightPx}px`);
  }

  async function runExpandSequence() {
    const gen = ++sequenceGen;
    anchorLocked = true;
    placeMenu({ lockEdge: true });
    phase = "toolbar";
    await tick();
    placeMenu({ lockEdge: true });
    if (gen !== sequenceGen) return;
    if (!prefersReducedMotion()) await sleep(CHROME_MS);
    if (gen !== sequenceGen) return;
    phase = "open";
    await tick();
    placeMenu({ lockEdge: true });
    if (gen !== sequenceGen) return;
    if (!prefersReducedMotion()) await sleep(HEIGHT_MS);
    if (gen !== sequenceGen) return;
    anchorLocked = false;
  }

  async function runCollapseSequence() {
    const gen = ++sequenceGen;
    anchorLocked = true;
    placeMenu({ lockEdge: true });
    // 1) Tuck the drawer first (full width bar stays).
    phase = "toolbar";
    await tick();
    placeMenu({ lockEdge: true });
    if (gen !== sequenceGen) return;
    if (!prefersReducedMotion()) await sleep(HEIGHT_MS);
    if (gen !== sequenceGen) return;
    // 2) Then retract the bar to super-compact.
    phase = "seed";
    await tick();
    placeMenu({ lockEdge: true });
    if (gen !== sequenceGen) return;
    if (!prefersReducedMotion()) await sleep(WIDTH_MS);
    if (gen !== sequenceGen) return;
    anchorLocked = false;
    placeMenu({ lockEdge: false });
  }

  function resetPhase() {
    sequenceGen += 1;
    anchorLocked = false;
    phase = "seed";
  }

  $effect(() => {
    if (!open) {
      resetPhase();
      lastTargetKey = "";
      return;
    }
    if (targetKey !== lastTargetKey) {
      lastTargetKey = targetKey;
      resetPhase();
      syncGeometryFromTrigger();
    }
  });

  $effect(() => {
    if (!open || !triggerEl) return;
    // Initial geometry when first opened (or trigger becomes available).
    if (phase === "seed" && !anchorLocked) {
      syncGeometryFromTrigger();
    }
  });

  $effect(() => {
    if (!open || !dockHost || !dockSlotEl) return;
    pushLmeDockHost(dockSlotEl);
    return () => {
      popLmeDockHost();
    };
  });

  $effect(() => {
    if (!open || !triggerEl || !menuEl) return;
    let frame = 0;
    const place = () => {
      if (!triggerEl || !menuEl) return;
      if (!anchorLocked) {
        // Resize may change available height; keep direction for the session.
        openHeightPx = railPopoverOpenHeightCap(triggerEl, expandDir, {
          pad: PLACE_PAD,
          maxHeight: OPEN_MAX_PX,
          cursor: cursorAnchor,
        });
      }
      placeMenu();
      frame = window.requestAnimationFrame(() => {
        placeMenu();
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

  function toggleExpanded(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    if (phase === "open") {
      void runCollapseSequence();
      return;
    }
    if (phase === "toolbar") {
      void runExpandSequence();
      return;
    }
    void runExpandSequence();
  }
</script>

{#if open}
  <div
    bind:this={menuEl}
    class="nav-rail-view-popover"
    class:nav-rail-view-popover--chrome={chromeExpanded}
    class:nav-rail-view-popover--open={listOpen}
    class:nav-rail-view-popover--up={expandUp}
    data-phase={phase}
    data-expand={expandDir}
    role="dialog"
    aria-label={title}
    data-debug-label="nav-rail-view-popover"
    style:--nav-rail-popover-open-height="{openHeightPx}px"
    onclick={(event) => event.stopPropagation()}
  >
    <div class="nav-rail-view-popover-toolbar">
      <div class="nav-rail-view-popover-actions">
        {#if toolbar}
          {@render toolbar()}
        {/if}
        {#if dockHost}
          <div
            bind:this={dockSlotEl}
            class="nav-rail-view-popover-dock-slot"
            data-debug-label="nav-rail-dock-slot"
          ></div>
        {/if}
      </div>
      {#if listOpen && onDockToRail}
        <button
          type="button"
          class="nav-rail-view-popover-expand nav-rail-view-popover-dock"
          title="Expand to side rail"
          aria-label="Expand {title} to side rail"
          onclick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onDockToRail();
          }}
        >
          <Expand size={14} strokeWidth={1.75} />
        </button>
      {/if}
      <button
        type="button"
        class="nav-rail-view-popover-expand"
        class:nav-rail-view-popover-expand-active={listOpen}
        title={listOpen ? `Hide ${title}` : `Show ${title}`}
        aria-label={listOpen ? `Hide ${title}` : `Show ${title}`}
        aria-expanded={listOpen}
        onclick={toggleExpanded}
      >
        <ChevronRight size={15} strokeWidth={2} />
      </button>
    </div>

    <!-- Keep children mounted so LME docks can portal into the toolbar. -->
    <div class="nav-rail-view-popover-body" aria-hidden={!listOpen}>
      <div class="nav-rail-view-popover-body-inner">
        {@render children()}
      </div>
    </div>
  </div>
{/if}

<style>
  .nav-rail-view-popover {
    --nav-rail-popover-seed-width: 7.1rem;
    --nav-rail-popover-full-width: min(22rem, calc(100vw - 2rem));
    /* Match icon btn (1.75rem) + equal vertical padding — keeps seed optically centered. */
    --nav-rail-popover-bar-height: 2.35rem;
    --nav-rail-popover-open-height: min(32rem, calc(100vh - 2rem));

    position: fixed;
    z-index: 80;
    display: flex;
    width: var(--nav-rail-popover-seed-width);
    height: var(--nav-rail-popover-bar-height);
    max-height: var(--nav-rail-popover-open-height);
    flex-direction: column;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-600) / 0.3);
    /* Keep radius constant — morphing pill→rect on open reads as a jump. */
    border-radius: 1rem;
    background: rgb(var(--color-surface-900) / 0.97);
    box-shadow:
      0 14px 36px rgb(0 0 0 / 0.36),
      0 0 0 1px rgb(255 255 255 / 0.03);
    backdrop-filter: blur(16px);
    transition:
      width 320ms cubic-bezier(0.16, 1, 0.3, 1),
      height 360ms cubic-bezier(0.16, 1, 0.3, 1),
      box-shadow 220ms ease;
    animation: nav-rail-view-popover-in 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  /* Toolbar stays on the trigger edge; list opens upward. */
  .nav-rail-view-popover--up {
    flex-direction: column-reverse;
  }

  .nav-rail-view-popover--chrome,
  .nav-rail-view-popover--open,
  .nav-rail-view-popover:has(.lme-dock-search-expand),
  .nav-rail-view-popover:has(.nav-rail-context-toolbar) {
    width: var(--nav-rail-popover-full-width);
  }

  .nav-rail-view-popover:has(.nav-rail-popover-toolbar-label) {
    width: fit-content;
    min-width: var(--nav-rail-popover-seed-width);
  }

  .nav-rail-view-popover--chrome:has(.nav-rail-popover-toolbar-label),
  .nav-rail-view-popover--open:has(.nav-rail-popover-toolbar-label) {
    width: var(--nav-rail-popover-full-width);
  }

  .nav-rail-view-popover--open {
    height: var(--nav-rail-popover-open-height);
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

  .nav-rail-view-popover-toolbar {
    display: flex;
    height: var(--nav-rail-popover-bar-height);
    flex-shrink: 0;
    align-items: center;
    gap: 0.1rem;
    box-sizing: border-box;
    padding: 0.2rem 0.25rem 0.2rem 0.3rem;
    border-bottom: 1px solid transparent;
    border-top: 1px solid transparent;
    transition: border-color 180ms ease;
  }

  .nav-rail-view-popover--open:not(.nav-rail-view-popover--up) .nav-rail-view-popover-toolbar {
    border-bottom-color: rgb(var(--color-surface-700) / 0.32);
  }

  .nav-rail-view-popover--open.nav-rail-view-popover--up .nav-rail-view-popover-toolbar {
    border-top-color: rgb(var(--color-surface-700) / 0.32);
  }

  .nav-rail-view-popover-actions {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    justify-content: flex-start;
    gap: 0.05rem;
    overflow: hidden;
  }

  .nav-rail-view-popover-dock-slot {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    align-self: center;
    height: 1.75rem;
  }

  .nav-rail-view-popover-dock-slot :global(.lme-side-rail-dock) {
    display: flex;
    width: 100%;
    height: 1.75rem;
    min-height: 0;
    min-width: 0;
    align-items: center;
    gap: 0.05rem;
    margin: 0;
    border: 0;
    padding: 0;
    background: transparent;
  }

  .nav-rail-view-popover-dock-slot :global(.lme-side-rail-dock--status) {
    flex: 1;
    height: 1.75rem;
    justify-content: flex-start;
  }

  /* Sparse docks: collapse leading spacers / ghost status so icons sit left. */
  .nav-rail-view-popover-dock-slot
    :global(.lme-side-rail-dock > .min-w-0.flex-1:first-child:empty),
  .nav-rail-view-popover-dock-slot
    :global(.lme-side-rail-dock > .min-w-1.flex-1:first-child:empty),
  .nav-rail-view-popover-dock-slot
    :global(.lme-side-rail-dock > .flex-1:first-child:empty),
  .nav-rail-view-popover-dock-slot
    :global(
      .lme-side-rail-dock > .min-w-0.flex-1:first-child:not(:has(.vault-dock-branch))
    ) {
    display: none;
  }

  .nav-rail-view-popover-dock-slot :global(.workshop-faint) {
    display: none;
  }

  /* Seed: only primary verbs (+ / search). Secondary chrome waits for bar extend. */
  .nav-rail-view-popover:not(.nav-rail-view-popover--chrome):not(.nav-rail-view-popover--open)
    :global(.lme-dock-chrome-secondary) {
    max-width: 0 !important;
    max-height: 0 !important;
    margin: 0 !important;
    padding: 0 !important;
    opacity: 0;
    transform: translateX(-0.3rem);
    pointer-events: none;
    overflow: hidden;
    border: 0 !important;
  }

  .nav-rail-view-popover--chrome :global(.lme-dock-chrome-secondary),
  .nav-rail-view-popover--open :global(.lme-dock-chrome-secondary) {
    max-width: 18rem;
    opacity: 1;
    transform: translateX(0);
    pointer-events: auto;
    transition:
      max-width 300ms cubic-bezier(0.16, 1, 0.3, 1),
      opacity 220ms ease 40ms,
      transform 300ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .nav-rail-view-popover--chrome :global(.lme-dock-chrome-secondary--spacer),
  .nav-rail-view-popover--open :global(.lme-dock-chrome-secondary--spacer) {
    max-width: none;
    flex: 1 1 auto;
    min-width: 0.35rem;
  }

  .nav-rail-view-popover-dock-slot :global(.lme-dock-chrome-secondary) {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.1rem;
    overflow: hidden;
    transition:
      max-width 280ms cubic-bezier(0.16, 1, 0.3, 1),
      opacity 200ms ease,
      transform 280ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .nav-rail-view-popover:not(.nav-rail-view-popover--chrome):not(.nav-rail-view-popover--open)
    :global(.lme-dock-chrome-secondary--spacer) {
    flex: 0 0 0;
    min-width: 0;
  }

  /* Notes breadcrumb: readable text, no junk icons. */
  .nav-rail-view-popover-dock-slot :global(.vault-dock-branch) {
    max-width: 5.75rem;
    color: rgb(var(--color-surface-100));
    font-size: 0.72rem;
    font-weight: 500;
    letter-spacing: -0.01em;
  }

  .nav-rail-view-popover-dock-slot :global(.vault-dock-branch__icon) {
    display: none;
  }

  .nav-rail-view-popover-dock-slot :global(.vault-dock-branch__label) {
    color: inherit;
  }

  .nav-rail-view-popover-dock-slot :global(.nav-rail-dock-crumb-sep) {
    color: rgb(var(--color-surface-500));
  }

  .nav-rail-view-popover-actions :global(.vault-dock-icon-btn),
  .nav-rail-view-popover-dock-slot :global(.vault-dock-icon-btn) {
    width: 1.75rem;
    height: 1.75rem;
    color: rgb(var(--color-surface-200));
  }

  .nav-rail-view-popover-actions :global(.vault-dock-icon-btn:hover),
  .nav-rail-view-popover-dock-slot :global(.vault-dock-icon-btn:hover) {
    color: rgb(var(--color-surface-50));
  }

  .nav-rail-view-popover-expand {
    display: inline-flex;
    width: 1.75rem;
    height: 1.75rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.4rem;
    color: rgb(var(--color-surface-400));
    transition:
      background-color 120ms ease,
      color 120ms ease;
  }

  .nav-rail-view-popover-expand:hover {
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
    color: rgb(var(--color-surface-100));
  }

  /* Keep `>` pointing right — no rotate, no filled "active" chrome. */
  .nav-rail-view-popover-expand-active {
    color: rgb(var(--color-surface-200));
  }

  .nav-rail-view-popover-dock {
    animation: nav-rail-view-popover-dock-in 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes nav-rail-view-popover-dock-in {
    from {
      opacity: 0;
      transform: scale(0.92);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  .nav-rail-view-popover-body {
    display: grid;
    min-height: 0;
    flex: 0 0 auto;
    grid-template-rows: 0fr;
    opacity: 0;
    transition:
      grid-template-rows 340ms cubic-bezier(0.16, 1, 0.3, 1),
      opacity 220ms ease;
  }

  .nav-rail-view-popover--open .nav-rail-view-popover-body {
    flex: 1 1 auto;
    grid-template-rows: 1fr;
    opacity: 1;
    transition:
      grid-template-rows 360ms cubic-bezier(0.16, 1, 0.3, 1) 40ms,
      opacity 240ms ease 60ms;
  }

  .nav-rail-view-popover-body-inner {
    display: flex;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
  }

  @media (prefers-reduced-motion: reduce) {
    .nav-rail-view-popover,
    .nav-rail-view-popover-body,
    .nav-rail-view-popover-dock-slot :global(.lme-dock-chrome-secondary) {
      transition: none !important;
    }
  }
</style>
