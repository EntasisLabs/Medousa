<script lang="ts">
  import {
    Activity,
    ArrowLeft,
    ArrowRight,
    CalendarClock,
    ChevronLeft,
    Eye,
    Hammer,
    History,
    Layers,
    ListFilter,
    Menu,
    MessageCircle,
    MessagesSquare,
    MoreHorizontal,
    OctagonX,
    Pencil,
    Play,
    Plus,
    RefreshCw,
    Save,
    Search,
    Sparkles,
    Square,
    Upload,
    UserRound,
    Wrench,
  } from "@lucide/svelte";
  import type { Component } from "svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
  import { mobileCodeWorkspaceState } from "$lib/stores/mobileCodeWorkspaceState.svelte";
  import { haptic } from "$lib/haptics";
  import { prepareTalkAboutNote } from "$lib/utils/vaultNoteBridge";
  import { openMobileCodeThread } from "$lib/utils/mobileCodeOpen";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import {
    mobileChromeLeading,
    mobileChromeTrailing,
    resolveMobileChromeSurface,
    type MobileChromeActionId,
  } from "$lib/utils/mobileTopChrome";
  import type { AutomationsChromeMode } from "$lib/stores/automationsNav.svelte";
  import {
    workshopBrandCssVars,
  } from "$lib/types/workshopRegistry";

  let sessionsMenuOpen = $state(false);

  const surface = $derived(
    resolveMobileChromeSurface(
      layout.mobileTab,
      layout.libraryView,
      layout.moreDestination,
    ),
  );

  $effect(() => {
    if (surface !== "chat" && sessionsMenuOpen) {
      sessionsMenuOpen = false;
    }
  });

  const automationsMode = $derived.by((): AutomationsChromeMode => {
    if (surface !== "automations") return "browse";
    // Live composer flag wins (most important editor surface).
    if (flows.composerOpen) return "flow-editor";
    return automationsNav.mobileChromeMode;
  });

  const leading = $derived(mobileChromeLeading(surface));
  const trailing = $derived(
    mobileChromeTrailing(
      surface,
      automationsNav.currentSection,
      automationsMode,
      mobileCodeWorkspaceState.chromeMode,
    ),
  );
  const brandStyle = $derived(workshopBrandCssVars(workshops.activeWorkshop?.brandColor));
  const notesFilterActive = $derived(
    vault.activeSpaceFilter !== null || vault.libraryBrowseMode !== "folders",
  );

  const icons: Partial<Record<MobileChromeActionId, Component>> = {
    menu: Menu,
    back: ChevronLeft,
    sessions: MessagesSquare,
    identity: UserRound,
    search: Search,
    notesFilter: ListFilter,
    newNote: Plus,
    noteEdit: Pencil,
    noteChat: MessageCircle,
    noteMore: MoreHorizontal,
    automationsFilter: ListFilter,
    newAutomation: Plus,
    scriptTools: Wrench,
    scriptSave: Save,
    scriptRun: Play,
    scriptCompile: Hammer,
    flowAddStep: Plus,
    flowPlan: Sparkles,
    flowRun: Play,
    flowSchedule: CalendarClock,
    flowClose: OctagonX,
    agentsFilter: ListFilter,
    agentsImport: Upload,
    browserTabs: Layers,
    browserBack: ArrowLeft,
    browserForward: ArrowRight,
    browserReload: RefreshCw,
    activity: Activity,
    codeSearch: Search,
    codeSave: Save,
    codeFind: Search,
    codeThread: MessageCircle,
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
        if (surface === "automations" && flows.composerOpen) {
          flows.closeComposer();
          return;
        }
        if (surface === "code") {
          if (mobileCodeWorkspaceState.handleBack()) return;
          layout.backToMoreHub();
          return;
        }
        if (
          surface === "more-nested" ||
          surface === "automations" ||
          surface === "agents"
        ) {
          layout.backToMoreHub();
          return;
        }
        layout.setLibraryView("list");
        return;
      case "sessions":
        // Handled by the sessions OverflowMenu (New chat / Previous sessions).
        return;
      case "identity":
        sessionsMenuOpen = false;
        layout.setSessionDrawerOpen(false);
        layout.toggleIdentityDrawer();
        return;
      case "search":
        if (surface === "automations") {
          window.dispatchEvent(new CustomEvent("medousa-mobile-automations-search-focus"));
          return;
        }
        if (surface === "agents") {
          window.dispatchEvent(new CustomEvent("medousa-mobile-agents-search-focus"));
          return;
        }
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
      case "automationsFilter":
        window.dispatchEvent(new CustomEvent("medousa-mobile-automations-filter"));
        return;
      case "newAutomation":
        window.dispatchEvent(new CustomEvent("medousa-mobile-automations-new"));
        return;
      case "scriptTools":
        window.dispatchEvent(new CustomEvent("medousa-mobile-automations-tools"));
        return;
      case "scriptSave":
        window.dispatchEvent(new CustomEvent("medousa-mobile-script-save"));
        return;
      case "scriptRun":
        window.dispatchEvent(new CustomEvent("medousa-mobile-script-run"));
        return;
      case "scriptCompile":
        window.dispatchEvent(new CustomEvent("medousa-mobile-script-compile"));
        return;
      case "flowAddStep":
        window.dispatchEvent(new CustomEvent("medousa-mobile-flow-add"));
        return;
      case "flowPlan":
        window.dispatchEvent(new CustomEvent("medousa-mobile-flow-plan"));
        return;
      case "flowRun":
        window.dispatchEvent(new CustomEvent("medousa-mobile-flow-run"));
        return;
      case "flowSchedule":
        window.dispatchEvent(new CustomEvent("medousa-mobile-flow-schedule"));
        return;
      case "flowClose":
        window.dispatchEvent(new CustomEvent("medousa-mobile-flow-close"));
        return;
      case "agentsFilter":
        window.dispatchEvent(new CustomEvent("medousa-mobile-agents-filter"));
        return;
      case "agentsImport":
        window.dispatchEvent(new CustomEvent("medousa-mobile-agents-import"));
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
      case "codeSearch":
        window.dispatchEvent(new CustomEvent("medousa-mobile-code-search"));
        return;
      case "codeSave":
        window.dispatchEvent(new CustomEvent("medousa-mobile-code-save"));
        return;
      case "codeFind":
        window.dispatchEvent(new CustomEvent("medousa-mobile-code-find"));
        return;
      case "codeThread":
        await openMobileCodeThread();
        return;
    }
  }

  function labelFor(id: MobileChromeActionId): string {
    switch (id) {
      case "menu":
        return "Open menu";
      case "back":
        if (surface === "automations" && flows.composerOpen) return "Back to flows";
        if (surface === "code" && mobileCodeWorkspaceState.inProject) {
          return "Back to code projects";
        }
        if (surface === "code") return "Back to Home";
        return surface === "more-nested" ||
          surface === "automations" ||
          surface === "agents"
          ? "Back to Home"
          : "Back to notes";
      case "workshop":
        return `Workshop — ${workshops.activeLabel}`;
      case "sessions":
        return "Sessions";
      case "identity":
        return "Open identity";
      case "search":
        if (surface === "automations") return "Search automations";
        if (surface === "agents") return "Search agents";
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
      case "automationsFilter":
        return "Automations section";
      case "newAutomation":
        return automationsNav.currentSection === "flows" ? "New flow" : "New schedule";
      case "scriptTools":
        return "Script tools";
      case "scriptSave":
        return "Save script";
      case "scriptRun":
        return "Run script";
      case "scriptCompile":
        return "Compile script";
      case "flowAddStep":
        return "Add step";
      case "flowPlan":
        return "Plan with AI";
      case "flowRun":
        return "Run flow";
      case "flowSchedule":
        return "Schedule flow";
      case "flowClose":
        return "Discard draft";
      case "agentsFilter":
        return "Filter agents";
      case "agentsImport":
        return "Import agents";
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
      case "codeSearch":
        return mobileCodeWorkspaceState.inProject ? "Search files" : "Search projects";
      case "codeSave":
        return "Save";
      case "codeFind":
        return "Find in file";
      case "codeThread":
        return "Open project thread";
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
      case "scriptSave":
        return graphemeScriptEditor.saveBusy || !graphemeScriptEditor.activeTab;
      case "codeSave": {
        const workId = mobileCodeWorkspaceState.selectedWorkId;
        const tab = workId ? codeWorkspace.activeFor(workId) : null;
        return !tab || !codeWorkspace.isDirty(tab) || Boolean(tab.preview);
      }
      case "codeThread":
        return !mobileCodeWorkspaceState.selectedWorkId;
      case "scriptRun":
        return workshop.runBusy || !graphemeScriptEditor.activeTab?.body.trim();
      case "scriptCompile":
        return (
          graphemeScriptEditor.compileBusy ||
          !graphemeScriptEditor.activeTab?.body.trim()
        );
      case "flowRun":
        return flows.running || flows.composerDraft.steps.length === 0;
      default:
        return false;
    }
  }

  async function createNewChat() {
    haptic("medium");
    sessionsMenuOpen = false;
    layout.setIdentityDrawerOpen(false);
    layout.setSessionDrawerOpen(false);
    await chat.newSession();
  }

  function openPreviousSessions() {
    haptic("light");
    sessionsMenuOpen = false;
    layout.setIdentityDrawerOpen(false);
    layout.setSessionDrawerOpen(true);
  }
</script>

<header class="mobile-top-chrome" data-chrome-surface={surface} data-automations-mode={automationsMode}>
  {#if leading}
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
  {:else}
    <span class="mobile-chrome-leading-spacer" aria-hidden="true"></span>
  {/if}

  <div class="mobile-chrome-actions">
    {#each trailing as action (action)}
      {#if action === "sessions"}
        <OverflowMenu
          bind:open={sessionsMenuOpen}
          align="right"
          label="Sessions"
          title="Sessions"
          panelWidth={12.5 * 16}
          panelClass="w-[12.5rem] rounded-xl border border-surface-500/40 bg-surface-900/95 p-1 shadow-xl backdrop-blur"
          onOpenChange={(open) => {
            if (open) haptic("light");
          }}
        >
          {#snippet trigger({ open, toggle })}
            <button
              type="button"
              class="mobile-chrome-icon"
              class:mobile-chrome-icon-active={open}
              aria-label="Sessions"
              title="Sessions"
              aria-expanded={open}
              aria-haspopup="menu"
              onclick={toggle}
            >
              <MessagesSquare size={18} strokeWidth={1.75} />
            </button>
          {/snippet}
          <button
            type="button"
            role="menuitem"
            class="vault-menu-item rounded-lg"
            onclick={() => void createNewChat()}
          >
            <Plus size={15} strokeWidth={1.75} class="shrink-0 opacity-70" />
            New chat
          </button>
          <button
            type="button"
            role="menuitem"
            class="vault-menu-item rounded-lg"
            onclick={openPreviousSessions}
          >
            <History size={15} strokeWidth={1.75} class="shrink-0 opacity-70" />
            Previous sessions
          </button>
        </OverflowMenu>
      {:else if action === "workshop"}
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
          class:text-content-link={action === "noteChat" ||
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
