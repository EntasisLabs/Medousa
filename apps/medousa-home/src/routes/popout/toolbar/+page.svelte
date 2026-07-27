<script lang="ts">
  import { onMount } from "svelte";
  import {
    Globe,
    House,
    LayoutGrid,
    MessageSquare,
    StickyNote,
    X,
  } from "@lucide/svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import { readLastViewPopoutSurface } from "$lib/utils/viewPopout";
  import {
    hideDesktopToolbar,
    isTauri,
    showBrowser,
    showChatPopout,
    showMainWindow,
    showVaultSticky,
    showViewPopout,
  } from "$lib/window";
  import { connectWorkshop } from "$lib/workshopConnection";
  import { whenDocumentVisible } from "$lib/utils/whenDocumentVisible";

  const RAIL_SIZE = { width: 64, height: 340 };
  const PICKER_SIZE = { width: 260, height: 340 };

  let viewsOpen = $state(false);

  const customViews = $derived(
    environment.navSurfaces().filter((surface) => surface.kind === "custom"),
  );

  onMount(() => {
    document.documentElement.classList.add("desktop-toolbar-shell");
    document.body.classList.add("desktop-toolbar-shell");
    void syncToolbarWindowSize(false);

    const detachWorkshop = whenDocumentVisible(() =>
      connectWorkshop({
        onHealthChange: () => {},
        mode: "observer",
      }),
    );
    return () => {
      document.documentElement.classList.remove("desktop-toolbar-shell");
      document.body.classList.remove("desktop-toolbar-shell");
      detachWorkshop();
    };
  });

  async function syncToolbarWindowSize(expanded: boolean) {
    if (!isTauri()) return;
    try {
      const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
      const size = expanded ? PICKER_SIZE : RAIL_SIZE;
      await getCurrentWindow().setSize(new LogicalSize(size.width, size.height));
    } catch {
      /* ignore */
    }
  }

  async function setViewsOpen(next: boolean) {
    viewsOpen = next;
    await syncToolbarWindowSize(next);
  }

  async function openChat() {
    await setViewsOpen(false);
    if (isTauri()) await showChatPopout();
  }

  async function openNote() {
    await setViewsOpen(false);
    if (isTauri()) await showVaultSticky();
  }

  async function openWeb() {
    await setViewsOpen(false);
    if (isTauri()) await showBrowser();
  }

  async function openMain() {
    await setViewsOpen(false);
    if (isTauri()) await showMainWindow();
  }

  async function openViews(event?: MouseEvent) {
    // Shift/Alt = always show the picker; plain click reopens the last view.
    if (event?.shiftKey || event?.altKey) {
      await setViewsOpen(!viewsOpen);
      return;
    }
    if (viewsOpen) {
      await setViewsOpen(false);
      return;
    }
    const last = readLastViewPopoutSurface();
    if (last && customViews.some((view) => view.id === last)) {
      await showViewPopout(last);
      return;
    }
    if (customViews.length === 1) {
      await showViewPopout(customViews[0].id);
      return;
    }
    await setViewsOpen(true);
  }

  async function pickView(surfaceId: string) {
    await setViewsOpen(false);
    await showViewPopout(surfaceId);
  }

  async function dismissToolbar() {
    await setViewsOpen(false);
    if (isTauri()) await hideDesktopToolbar();
  }
</script>

<div class="desktop-toolbar-root">
  <div class="desktop-toolbar-rail" data-tauri-drag-region>
    <button
      type="button"
      class="desktop-toolbar-btn"
      title="Chat"
      aria-label="Open chat window"
      onclick={() => void openChat()}
    >
      <MessageSquare size={18} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="desktop-toolbar-btn"
      title="Note"
      aria-label="Open note window"
      onclick={() => void openNote()}
    >
      <StickyNote size={18} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="desktop-toolbar-btn"
      title="Web"
      aria-label="Open web window"
      onclick={() => void openWeb()}
    >
      <Globe size={18} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="desktop-toolbar-btn"
      class:desktop-toolbar-btn-active={viewsOpen}
      title="Views — click last, Shift-click list"
      aria-label="Open custom views"
      aria-expanded={viewsOpen}
      onclick={(event) => void openViews(event)}
    >
      <LayoutGrid size={18} strokeWidth={1.75} />
    </button>

    <span class="desktop-toolbar-spacer" aria-hidden="true"></span>

    <button
      type="button"
      class="desktop-toolbar-btn"
      title="Main window"
      aria-label="Show main Medousa window"
      onclick={() => void openMain()}
    >
      <House size={18} strokeWidth={1.75} />
    </button>
    {#if isTauri()}
      <button
        type="button"
        class="desktop-toolbar-btn desktop-toolbar-btn-quiet"
        title="Hide toolbar"
        aria-label="Hide desktop toolbar"
        onclick={() => void dismissToolbar()}
      >
        <X size={16} strokeWidth={1.75} />
      </button>
    {/if}
  </div>

  {#if viewsOpen}
    <div class="desktop-toolbar-views" role="listbox" aria-label="Custom views">
      {#if customViews.length === 0}
        <p class="desktop-toolbar-empty">No custom views yet.</p>
      {:else}
        {#each customViews as surface (surface.id)}
          {@const SurfaceIcon = environmentIcon(surface.icon)}
          <button
            type="button"
            role="option"
            class="desktop-toolbar-view-row"
            onclick={() => void pickView(surface.id)}
          >
            <SurfaceIcon size={14} strokeWidth={1.75} />
            <span class="truncate">{surface.label}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  :global(html.desktop-toolbar-shell),
  :global(body.desktop-toolbar-shell),
  :global(html.desktop-toolbar-shell body),
  :global(.desktop-toolbar-shell .h-full) {
    background: transparent !important;
    background-color: transparent !important;
    overflow: hidden !important;
  }

  .desktop-toolbar-root {
    display: flex;
    height: 100vh;
    width: 100vw;
    align-items: stretch;
    gap: 0.45rem;
    padding: 0.4rem;
    box-sizing: border-box;
    background: transparent;
    -webkit-user-select: none;
    user-select: none;
  }

  .desktop-toolbar-rail {
    display: flex;
    width: 3.1rem;
    flex-direction: column;
    align-items: center;
    gap: 0.2rem;
    border-radius: 1rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-950) / 0.92);
    backdrop-filter: blur(16px) saturate(1.1);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.35);
    padding: 0.45rem 0.3rem;
  }

  .desktop-toolbar-btn {
    display: inline-flex;
    width: 2.35rem;
    height: 2.35rem;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 0.65rem;
    background: transparent;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
  }

  .desktop-toolbar-btn:hover,
  .desktop-toolbar-btn-active {
    background: rgb(var(--color-surface-800) / 0.9);
    color: rgb(var(--color-surface-50));
  }

  .desktop-toolbar-btn-quiet {
    color: rgb(var(--color-surface-500));
  }

  .desktop-toolbar-spacer {
    flex: 1 1 auto;
    min-height: 0.5rem;
  }

  .desktop-toolbar-views {
    min-width: 0;
    flex: 1 1 auto;
    overflow: auto;
    border-radius: 0.9rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-950) / 0.92);
    backdrop-filter: blur(16px) saturate(1.1);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.35);
    padding: 0.4rem;
  }

  .desktop-toolbar-empty {
    margin: 0;
    padding: 0.65rem 0.55rem;
    font-size: 11px;
    line-height: 1.4;
    color: rgb(var(--color-surface-500));
  }

  .desktop-toolbar-view-row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.45rem;
    border: 0;
    border-radius: 0.55rem;
    background: transparent;
    padding: 0.45rem 0.5rem;
    color: rgb(var(--color-surface-100));
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }

  .desktop-toolbar-view-row:hover {
    background: rgb(var(--color-surface-800) / 0.85);
  }
</style>
