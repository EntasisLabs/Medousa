<script lang="ts">
  /**
   * Titlebar “+” menu — paginated create list (≈5 rows: items + up/down).
   */
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { calendar } from "$lib/stores/calendar.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { peersShell } from "$lib/stores/peersShell.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { dispatchBrowserFocusUrl } from "$lib/utils/browserChromeEvents";
  import { placeToolbarPopover } from "$lib/utils/railPopover";
  import { dispatchWorkFocusAsk } from "$lib/utils/workChromeEvents";
  import {
    BookOpen,
    Bot,
    CalendarClock,
    CalendarDays,
    ChevronDown,
    ChevronUp,
    FileCode2,
    GitBranch,
    Globe,
    MessageSquare,
    MessageSquarePlus,
    Users,
  } from "@lucide/svelte";
  import { tick, type Component, type Snippet } from "svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  type NewTabKind =
    | "chat"
    | "note"
    | "web"
    | "ask"
    | "calendar"
    | "peer"
    | "script"
    | "agent"
    | "flow"
    | "schedule";

  type NewTabItem = {
    id: NewTabKind;
    label: string;
    icon: Component<{ size?: number; strokeWidth?: number }>;
  };

  const ITEMS: NewTabItem[] = [
    { id: "chat", label: "Chat", icon: MessageSquare },
    { id: "note", label: "Note", icon: BookOpen },
    { id: "web", label: "Web", icon: Globe },
    { id: "ask", label: "Ask", icon: MessageSquarePlus },
    { id: "calendar", label: "Calendar", icon: CalendarDays },
    { id: "peer", label: "Peer", icon: Users },
    { id: "script", label: "Script", icon: FileCode2 },
    { id: "agent", label: "Agent", icon: Bot },
    { id: "flow", label: "Flow", icon: GitBranch },
    { id: "schedule", label: "Schedule", icon: CalendarClock },
  ];

  /** Fixed pages matching the slide-window pagination sketch. */
  const PAGES: Array<{ ids: NewTabKind[]; showUp: boolean; showDown: boolean }> = [
    { ids: ["chat", "note", "web", "ask"], showUp: false, showDown: true },
    { ids: ["calendar", "peer", "script"], showUp: true, showDown: true },
    { ids: ["agent", "flow", "schedule"], showUp: true, showDown: false },
  ];

  let open = $state(false);
  let page = $state(0);
  let slideDir = $state<"down" | "up">("down");
  let busy = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  const pageDef = $derived(PAGES[page] ?? PAGES[0]!);
  const pageItems = $derived(
    pageDef.ids
      .map((id) => ITEMS.find((item) => item.id === id))
      .filter((item): item is NewTabItem => Boolean(item)),
  );

  $effect(() => {
    if (!open || !triggerEl || !menuEl) return;
    let frame = 0;
    const place = () => {
      if (!triggerEl || !menuEl) return;
      placeToolbarPopover(triggerEl, menuEl, {
        prefer: "below",
        width: 11.5 * 16,
        gap: 6,
        pad: 8,
      });
      frame = window.requestAnimationFrame(() => {
        if (!triggerEl || !menuEl) return;
        placeToolbarPopover(triggerEl, menuEl, {
          prefer: "below",
          width: 11.5 * 16,
          gap: 6,
          pad: 8,
        });
      });
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

  function toggle() {
    open = !open;
    if (open) {
      page = 0;
      slideDir = "down";
    }
  }

  function close() {
    open = false;
  }

  function goDown() {
    if (page >= PAGES.length - 1) return;
    slideDir = "down";
    page += 1;
  }

  function goUp() {
    if (page <= 0) return;
    slideDir = "up";
    page -= 1;
  }

  async function choose(kind: NewTabKind) {
    if (busy) return;
    busy = true;
    try {
      close();
      switch (kind) {
        case "chat":
          await chat.newSession();
          break;
        case "note":
          vault.openNewNoteDialog();
          break;
        case "web":
          await humanBrowser.openTab("about:blank");
          dispatchBrowserFocusUrl();
          break;
        case "ask":
          shellTabs.openSurface("work", { activate: true });
          await tick();
          dispatchWorkFocusAsk();
          break;
        case "calendar":
          shellTabs.openSurface("calendar", { activate: true });
          calendar.openCreate();
          break;
        case "peer":
          shellTabs.openSurface("peers", { activate: true });
          peersShell.requestAddPeer();
          break;
        case "script":
          lmeWorkspace.openNewScript();
          break;
        case "agent": {
          const detail = await catalog.createManuscript({ name: "New agent" });
          lmeWorkspace.openManuscript(detail.id, detail.name);
          break;
        }
        case "flow":
          lmeWorkspace.openNewFlow();
          break;
        case "schedule":
          // Create UI lives on AutomationsPanel — open the automations rail view.
          lmeWorkspace.setExplorerMode("schedules");
          layout.openShellSidebarView("automations");
          automationDraft.openCreate();
          break;
      }
    } catch (err) {
      console.error("New tab create failed", err);
    } finally {
      busy = false;
    }
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<button
  bind:this={triggerEl}
  type="button"
  class="app-titlebar-btn"
  class:app-titlebar-btn--open={open}
  title="New tab"
  aria-label="New tab"
  aria-haspopup="menu"
  aria-expanded={open}
  disabled={busy}
  onclick={toggle}
>
  {@render children()}
</button>

{#if open}
  <BodyPortal>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="new-tab-menu-scrim"
      role="presentation"
      onclick={close}
    ></div>
    <div
      bind:this={menuEl}
      class="new-tab-menu workshop-rail-sheet"
      role="menu"
      aria-label="Create new tab"
      onclick={(event) => event.stopPropagation()}
    >
      <div class="new-tab-menu-window" data-slide={slideDir}>
        {#key page}
          <div class="new-tab-menu-page" data-slide={slideDir}>
            {#if pageDef.showUp}
              <button
                type="button"
                class="new-tab-menu-nav"
                role="menuitem"
                title="Previous"
                aria-label="Previous page"
                onclick={goUp}
              >
                <ChevronUp size={15} strokeWidth={2} aria-hidden="true" />
              </button>
            {/if}

            {#each pageItems as item (item.id)}
              {@const Icon = item.icon}
              <button
                type="button"
                class="new-tab-menu-item"
                role="menuitem"
                onclick={() => void choose(item.id)}
              >
                <span class="new-tab-menu-item-icon" aria-hidden="true">
                  <Icon size={15} strokeWidth={1.75} />
                </span>
                <span class="new-tab-menu-item-label">{item.label}</span>
              </button>
            {/each}

            {#if pageDef.showDown}
              <button
                type="button"
                class="new-tab-menu-nav"
                role="menuitem"
                title="More"
                aria-label="Next page"
                onclick={goDown}
              >
                <ChevronDown size={15} strokeWidth={2} aria-hidden="true" />
              </button>
            {/if}
          </div>
        {/key}
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .new-tab-menu-scrim {
    position: fixed;
    inset: 0;
    z-index: 140;
  }

  .new-tab-menu {
    z-index: 145;
    width: 11.5rem;
    padding: 0.3rem;
    overflow: hidden;
  }

  .new-tab-menu-window {
    position: relative;
    overflow: hidden;
  }

  .new-tab-menu-page {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    animation: new-tab-menu-slide 180ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  .new-tab-menu-page[data-slide="down"] {
    animation-name: new-tab-menu-slide-down;
  }

  .new-tab-menu-page[data-slide="up"] {
    animation-name: new-tab-menu-slide-up;
  }

  @keyframes new-tab-menu-slide-down {
    from {
      opacity: 0;
      transform: translateY(-0.45rem);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes new-tab-menu-slide-up {
    from {
      opacity: 0;
      transform: translateY(0.45rem);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .new-tab-menu-item,
  .new-tab-menu-nav {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.55rem;
    margin: 0;
    padding: 0.45rem 0.55rem;
    border: 0;
    border-radius: 0.45rem;
    background: transparent;
    color: rgb(var(--color-surface-200));
    font: inherit;
    font-size: 0.8125rem;
    font-weight: 500;
    letter-spacing: -0.01em;
    text-align: left;
    cursor: pointer;
    transition:
      background-color 120ms ease,
      color 120ms ease;
  }

  .new-tab-menu-nav {
    justify-content: center;
    color: rgb(var(--color-surface-500));
    padding-block: 0.35rem;
  }

  .new-tab-menu-item:hover,
  .new-tab-menu-nav:hover {
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--color-surface-50));
  }

  .new-tab-menu-item-icon {
    display: inline-flex;
    width: 1rem;
    height: 1rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    color: rgb(var(--color-surface-400));
  }

  .new-tab-menu-item:hover .new-tab-menu-item-icon {
    color: rgb(var(--color-surface-200));
  }

  .new-tab-menu-item-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.app-titlebar-btn--open) {
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--color-surface-100));
  }

  @media (prefers-reduced-motion: reduce) {
    .new-tab-menu-page {
      animation: none;
    }
  }
</style>
