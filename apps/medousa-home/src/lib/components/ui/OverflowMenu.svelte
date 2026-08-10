<script lang="ts">
  import { Ellipsis } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import { onDestroy, tick } from "svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { placeToolbarPopover } from "$lib/utils/railPopover";

  interface Props {
    /** Controlled / bindable open state. */
    open?: boolean;
    /** Align the panel to the trigger's left or right edge. */
    align?: "left" | "right";
    /** Extra classes on the floating panel (width, theme). */
    panelClass?: string;
    /** Extra classes on the relative wrapper. */
    class?: string;
    /** Accessible name for the default ellipsis trigger. */
    label?: string;
    /** Title / tooltip for the default ellipsis trigger. */
    title?: string;
    /** Preferred panel width in px before viewport clamp. */
    panelWidth?: number;
    /** Optional custom trigger. Receives open state and toggle. */
    trigger?: Snippet<[{ open: boolean; toggle: () => void }]>;
    /** Menu body — put role=menuitem buttons inside. */
    children: Snippet;
    onOpenChange?: (open: boolean) => void;
  }

  let {
    open = $bindable(false),
    align = "right",
    panelClass = "w-44 rounded-md border border-surface-500/40 bg-surface-900 p-1 shadow-xl",
    class: wrapperClass = "",
    label = "More actions",
    title = "More actions",
    panelWidth = 11 * 16,
    trigger,
    children,
    onOpenChange,
  }: Props = $props();

  let rootEl = $state<HTMLDivElement | null>(null);
  let triggerEl = $state<HTMLElement | null>(null);
  let panelEl = $state<HTMLDivElement | null>(null);
  let focusedIndex = $state(-1);

  function setOpen(next: boolean) {
    if (open === next) return;
    open = next;
    onOpenChange?.(next);
    if (!next) focusedIndex = -1;
  }

  function toggle() {
    setOpen(!open);
  }

  function menuItems(): HTMLElement[] {
    if (!panelEl) return [];
    return Array.from(
      panelEl.querySelectorAll<HTMLElement>(
        '[role="menuitem"]:not([disabled]), [role="menuitemcheckbox"]:not([disabled])',
      ),
    ).filter((el) => !el.hasAttribute("disabled") && el.getAttribute("aria-disabled") !== "true");
  }

  function focusItem(index: number) {
    const items = menuItems();
    if (items.length === 0) return;
    const next = ((index % items.length) + items.length) % items.length;
    focusedIndex = next;
    items[next]?.focus();
  }

  function onPanelKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      focusItem(focusedIndex + 1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      focusItem(focusedIndex <= 0 ? menuItems().length - 1 : focusedIndex - 1);
      return;
    }
    if (event.key === "Home") {
      event.preventDefault();
      focusItem(0);
      return;
    }
    if (event.key === "End") {
      event.preventDefault();
      focusItem(menuItems().length - 1);
    }
  }

  function resolveTrigger(): HTMLElement | null {
    if (triggerEl) return triggerEl;
    if (!rootEl) return null;
    return rootEl.querySelector<HTMLElement>("[aria-haspopup='menu']");
  }

  function placePanel() {
    const triggerNode = resolveTrigger();
    if (!triggerNode || !panelEl) return;
    const measured = panelEl.offsetWidth || panelEl.getBoundingClientRect().width;
    placeToolbarPopover(triggerNode, panelEl, {
      prefer: "below",
      align: align === "left" ? "start" : "end",
      width: Math.max(panelWidth, measured || 0),
      gap: 6,
      pad: 8,
    });
    panelEl.style.overflowY = "auto";
  }

  $effect(() => {
    if (!open) return;
    let frame = 0;
    const place = () => {
      placePanel();
      frame = window.requestAnimationFrame(() => placePanel());
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

  $effect(() => {
    if (!open) return;
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setOpen(false);
      }
    };
    // Defer so the opening click does not immediately dismiss.
    const timer = window.setTimeout(() => {
      document.addEventListener("keydown", onKeydown, true);
      const items = menuItems();
      if (items.length > 0) {
        focusedIndex = 0;
        items[0]?.focus();
      }
    }, 0);
    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("keydown", onKeydown, true);
    };
  });

  onDestroy(() => {
    open = false;
  });
</script>

<div class="relative {wrapperClass}" bind:this={rootEl} data-overflow-menu>
  {#if trigger}
    {@render trigger({ open, toggle })}
  {:else}
    <button
      type="button"
      class="scripts-workbench-toolbar-btn"
      class:scripts-workbench-toolbar-btn-active={open}
      aria-label={label}
      {title}
      aria-expanded={open}
      aria-haspopup="menu"
      onclick={toggle}
      bind:this={triggerEl}
    >
      <Ellipsis size={15} strokeWidth={1.75} />
    </button>
  {/if}

  {#if open}
    <BodyPortal>
      <button
        type="button"
        class="fixed inset-0 z-40 cursor-default"
        aria-label="Close menu"
        tabindex="-1"
        onclick={() => setOpen(false)}
      ></button>
      <div
        bind:this={panelEl}
        class="z-50 {panelClass}"
        role="menu"
        tabindex="-1"
        onkeydown={onPanelKeydown}
      >
        {@render children()}
      </div>
    </BodyPortal>
  {/if}
</div>
