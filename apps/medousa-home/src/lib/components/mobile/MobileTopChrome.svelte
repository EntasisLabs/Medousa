<script lang="ts">
  import {
    Activity,
    ArrowLeft,
    ArrowRight,
    ChevronLeft,
    Eye,
    History,
    Layers,
    ListFilter,
    Menu,
    MessageCircle,
    MoreHorizontal,
    Pencil,
    Plus,
    RefreshCw,
    Search,
    Square,
    UserRound,
  } from "@lucide/svelte";
  import type { Component } from "svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { haptic } from "$lib/haptics";
  import { prepareTalkAboutNote } from "$lib/utils/vaultNoteBridge";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import {
    mobileChromeLeading,
    mobileChromeTrailing,
    resolveMobileChromeSurface,
    type MobileChromeActionId,
  } from "$lib/utils/mobileTopChrome";
  import {
    workshopBrandCssVars,
  } from "$lib/types/workshopRegistry";

  const surface = $derived(
    resolveMobileChromeSurface(
      layout.mobileTab,
      layout.libraryView,
      layout.moreDestination,
    ),
  );
  const leading = $derived(mobileChromeLeading(surface));
  const trailing = $derived(mobileChromeTrailing(surface));
  const brandStyle = $derived(workshopBrandCssVars(workshops.activeWorkshop?.brandColor));
  const notesFilterActive = $derived(
    vault.activeSpaceFilter !== null || vault.libraryBrowseMode !== "folders",
  );

  const icons: Partial<Record<MobileChromeActionId, Component>> = {
    menu: Menu,
    back: ChevronLeft,
    sessions: History,
    identity: UserRound,
    search: Search,
    notesFilter: ListFilter,
    newNote: Plus,
    noteEdit: Pencil,
    noteChat: MessageCircle,
    noteMore: MoreHorizontal,
    browserTabs: Layers,
    browserBack: ArrowLeft,
    browserForward: ArrowRight,
    browserReload: RefreshCw,
    activity: Activity,
  };

  function openMenu() {
    haptic("light");
    layout.openMobileDestinationsMenu();
  }

  async function run(id: MobileChromeActionId, button?: HTMLButtonElement | null) {
    haptic("light");
    switch (id) {
      case "menu":
        openMenu();
        return;
      case "workshop":
        openMenu();
        return;
      case "back":
        if (surface === "more-nested") {
          layout.backToMoreHub();
          return;
        }
        layout.setLibraryView("list");
        return;
      case "sessions":
        layout.setIdentityDrawerOpen(false);
        layout.toggleSessionDrawer();
        return;
      case "identity":
        layout.setSessionDrawerOpen(false);
        layout.toggleIdentityDrawer();
        return;
      case "search":
        window.dispatchEvent(new CustomEvent("medousa-mobile-notes-search-focus"));
        return;
      case "notesFilter":
        window.dispatchEvent(new CustomEvent("medousa-mobile-notes-filter"));
        return;
      case "newNote":
        vault.openNewNoteDialog();
        return;
      case "noteEdit":
        if (vault.editorMode === "preview") {
          vault.enterEditMode();
        } else {
          vault.enterPreviewMode();
        }
        return;
      case "noteChat": {
        if (!vault.selectedPath) return;
        if (vault.dirty) await vault.flushSave();
        const { scope, draft } = prepareTalkAboutNote(
          vault.selectedPath,
          vault.title,
          vault.content,
          vault.wikilinksOut,
          vault.backlinks,
        );
        chat.prefillFromVaultNote(scope, draft, { pin: true });
        switchMobileTab("chat");
        return;
      }
      case "noteMore":
        vault.openNoteActions();
        return;
      case "browserTabs": {
        const anchorRect = button?.getBoundingClientRect() ?? null;
        window.dispatchEvent(
          new CustomEvent("medousa-mobile-browser-tabs", {
            detail: { anchorRect },
          }),
        );
        return;
      }
      case "browserBack":
        await humanBrowser.goBack();
        return;
      case "browserForward":
        await humanBrowser.goForward();
        return;
      case "browserReload":
        if (humanBrowser.loading) {
          await humanBrowser.stop();
        } else {
          await humanBrowser.reload();
        }
        return;
      case "activity":
        layout.toggleActivitySheet();
        return;
    }
  }

  function labelFor(id: MobileChromeActionId): string {
    switch (id) {
      case "menu":
        return "Open menu";
      case "back":
        return surface === "more-nested" ? "Back to More" : "Back to notes";
      case "workshop":
        return `Workshop — ${workshops.activeLabel}`;
      case "sessions":
        return "Session history";
      case "identity":
        return "Open identity";
      case "search":
        return "Search notes";
      case "notesFilter":
        return "Browse filters";
      case "newNote":
        return "New note";
      case "noteEdit":
        return vault.editorMode === "preview" ? "Edit note" : "Preview note";
      case "noteChat":
        return "Talk about this note";
      case "noteMore":
        return "Note actions";
      case "browserTabs":
        return "Browser tabs";
      case "browserBack":
        return "Back";
      case "browserForward":
        return "Forward";
      case "browserReload":
        return humanBrowser.loading ? "Stop loading" : "Reload";
      case "activity":
        return "Activity";
    }
  }

  function isDisabled(id: MobileChromeActionId): boolean {
    switch (id) {
      case "noteEdit":
      case "noteChat":
        return vault.noteLoading || !vault.selectedPath;
      case "browserBack":
        return !humanBrowser.canGoBack;
      case "browserForward":
        return !humanBrowser.canGoForward;
      default:
        return false;
    }
  }
</script>

<header class="mobile-top-chrome" data-chrome-surface={surface}>
  <button
    type="button"
    class="mobile-chrome-icon"
    aria-label={labelFor(leading)}
    onclick={() => void run(leading)}
  >
    {#if leading === "back"}
      <ChevronLeft size={20} strokeWidth={1.75} />
    {:else}
      <Menu size={18} strokeWidth={1.75} />
    {/if}
  </button>

  <div class="mobile-chrome-actions">
    {#each trailing as action (action)}
      {#if action === "workshop"}
        <button
          type="button"
          class="mobile-chrome-icon mobile-chrome-workshop"
          style={brandStyle}
          aria-label={labelFor(action)}
          onclick={() => void run(action)}
        >
          <span class="mobile-chrome-workshop-mono">{workshops.activeMonogram}</span>
        </button>
      {:else}
        {@const Icon =
          action === "browserReload" && humanBrowser.loading
            ? Square
            : action === "noteEdit" && vault.editorMode === "edit"
              ? Eye
              : icons[action]}
        <button
          type="button"
          class="mobile-chrome-icon"
          class:text-primary-300={action === "noteChat" ||
            (action === "noteEdit" && vault.editorMode === "edit") ||
            (action === "notesFilter" && notesFilterActive)}
          aria-label={labelFor(action)}
          aria-pressed={action === "noteEdit" ? vault.editorMode === "edit" : undefined}
          disabled={isDisabled(action)}
          data-browser-popover-trigger={action === "browserTabs" ? "" : undefined}
          onclick={(event) => void run(action, event.currentTarget)}
        >
          {#if Icon}
            <Icon
              size={action === "browserReload" && humanBrowser.loading ? 12 : 18}
              strokeWidth={action === "browserReload" && humanBrowser.loading ? 2.25 : 1.75}
            />
          {/if}
        </button>
      {/if}
    {/each}
  </div>
</header>
