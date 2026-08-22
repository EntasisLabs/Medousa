<script lang="ts">
  import "$lib/styles/command-spotlight.postcss";
  import { buildWorkshopCommandContext } from "$lib/commands/context";
  import {
    collectWorkshopCommands,
    collectWorkshopLensCommands,
    parseSpotlightQuery,
  } from "$lib/commands/collectCommands";
  import type { SpotlightLens } from "$lib/commands/collectCommands";
  import { jumpPinSlot } from "$lib/commands/pinCommands";
  import { executeWorkshopCommand } from "$lib/commands/runWorkshopCommand";
  import { buildWorkspaceCommands } from "$lib/commands/registry";
  import { getVaultNote } from "$lib/daemon";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { sessionExportPreview } from "$lib/stores/sessionExportPreview.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { spotlightPins } from "$lib/stores/spotlightPins.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { CommandPreview, WorkshopCommand } from "$lib/commands/types";
  import type { ShellDesktopLayout, ShellTab } from "$lib/types/shellTabs";
  import {
    popBrowserPopoverOverlay,
    pushBrowserPopoverOverlay,
  } from "$lib/utils/browserPopoverOverlay";
  import { formatShortcut } from "$lib/platform";
  import { loadVaultExportPreviewModal } from "$lib/runtime/viewLoaders";
  import { leafOrder } from "$lib/utils/shellSplitTree";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import SpotlightWorkspacePreview from "./SpotlightWorkspacePreview.svelte";
  import {
    Bot,
    Boxes,
    BriefcaseBusiness,
    FileText,
    FlaskConical,
    FolderOpen,
    GitBranch,
    Globe2,
    House,
    LayoutPanelTop,
    MessageSquare,
    PanelRightOpen,
    Pin,
    Play,
    Plus,
    Search,
    Settings,
    TerminalSquare,
    Zap,
  } from "@lucide/svelte";
  import type { Component } from "svelte";

  interface Props {
    onFocusChat?: () => void;
  }

  interface SpotlightGroup {
    id: string;
    label: string;
    commands: WorkshopCommand[];
  }

  let { onFocusChat }: Props = $props();

  let query = $state("");
  let highlightIndex = $state(0);
  let busy = $state(false);
  let inputEl = $state<HTMLInputElement | null>(null);
  let promptValue = $state("");
  let selectedScopeId = $state("home");
  let selectedLens = $state<SpotlightLens>("overview");
  let groups = $state<SpotlightGroup[]>([]);
  let previewText = $state<string | null>(null);
  let previewTitle = $state<string | null>(null);
  let resultsEl = $state<HTMLDivElement | null>(null);
  let highlightNavigation = $state<"keyboard" | "pointer" | "data">("data");
  const notesMode = $derived(commandSpotlight.mode === "notes");
  const promptStep = $derived(commandSpotlight.promptStep);

  const ctx = $derived(
    buildWorkshopCommandContext({
      close: () => {
        commandSpotlight.rememberQuery(query, commandSpotlight.mode);
        commandSpotlight.closeSpotlight();
      },
      focusChat: () => onFocusChat?.(),
    }),
  );

  const flatCommands = $derived(groups.flatMap((group) => group.commands));
  const activeCommand = $derived(flatCommands[highlightIndex] ?? null);
  const activePreviewKind = $derived(activeCommand?.preview?.kind ?? "fallback");
  const activeGroup = $derived(
    activeCommand
      ? groups.find((group) => group.commands.some((command) => command.id === activeCommand.id)) ??
          null
      : null,
  );

  const scopeTabs = $derived([
    { id: "home", label: "Home" },
    ...shellTabs.desktops.map((desktop) => ({ id: desktop.id, label: desktop.name })),
  ]);

  const lensTabs: Array<{ id: SpotlightLens; label: string }> = [
    { id: "overview", label: "Overview" },
    { id: "recent", label: "Recent" },
    { id: "create", label: "Create" },
    { id: "actions", label: "Actions" },
  ];

  const selectedDesktop = $derived(
    selectedScopeId === "home"
      ? null
      : shellTabs.desktops.find((desktop) => desktop.id === selectedScopeId) ?? null,
  );

  const selectedDesktopLayout = $derived.by((): ShellDesktopLayout | null => {
    const desktop = selectedDesktop;
    if (!desktop) return null;
    if (desktop.id !== shellTabs.activeDesktopId) return desktop.layout;
    return {
      tabs: shellTabs.tabs,
      groups: shellTabs.groups,
      splitRoot: shellTabs.splitRoot,
      activeGroupId: shellTabs.activeGroupId,
      zoomedGroupId: shellTabs.zoomedGroupId,
    };
  });

  const hasRichStage = $derived(
    Boolean(selectedDesktopLayout && activeCommand) || activePreviewKind !== "fallback",
  );
  const showStage = $derived(hasRichStage || activeCommand?.risk === "attention");

  const activeWorkspaceTabId = $derived(
    activeCommand?.id.startsWith("spotlight-tab:")
      ? activeCommand.id.split(":").slice(2).join(":")
      : null,
  );

  const chatPreviewCommands = $derived(
    flatCommands
      .filter((command) => command.preview?.kind === "chat")
      .slice(0, 4),
  );

  const placeholder = $derived(
    notesMode
      ? "Search notes…"
      : selectedScopeId === "home"
        ? "Search this workshop…"
        : `Search ${selectedDesktop?.name ?? "workspace"}…`,
  );

  function tabKindLabel(tab: ShellTab): string {
    if (tab.kind === "lme") {
      const lme = lmeWorkspace.tabs.find((entry) => entry.tabId === tab.lmeTabId);
      return lme?.kind === "note" ? "Note" : lme?.kind === "code" ? "Code" : "Document";
    }
    if (tab.kind === "surface") return tab.surfaceId;
    return tab.kind === "chat" ? "Chat" : tab.kind[0]!.toUpperCase() + tab.kind.slice(1);
  }

  function iconForCommand(command: WorkshopCommand): Component | null {
    const identity = `${command.id} ${command.label} ${command.keywords ?? ""}`.toLowerCase();
    if (command.id.startsWith("spotlight-pane:")) return LayoutPanelTop;
    if (command.preview?.kind === "note") return FileText;
    if (command.preview?.kind === "chat") return MessageSquare;
    if (command.preview?.kind === "script") return Play;
    if (identity.includes("terminal")) return TerminalSquare;
    if (identity.includes("settings")) return Settings;
    if (identity.includes("agent")) return Bot;
    if (identity.includes("automation")) return Zap;
    if (identity.includes("artifact")) return Boxes;
    if (identity.includes("test")) return FlaskConical;
    if (
      identity.includes("git") ||
      identity.includes("changes") ||
      identity.includes("blame") ||
      identity.includes("fetch") ||
      identity.includes("pull") ||
      identity.includes("push") ||
      identity.includes("sync") ||
      identity.includes("review")
    ) {
      return GitBranch;
    }
    if (
      identity.includes("panel") ||
      identity.includes("split") ||
      identity.includes("preview") ||
      identity.includes("toolbar")
    ) {
      return PanelRightOpen;
    }
    if (identity.includes("browser") || identity.includes(" web ")) return Globe2;
    if (identity.includes("search") || identity.includes("find")) return Search;
    if (identity.includes("file") || identity.includes("folder")) return FolderOpen;
    if (identity.includes("work") || identity.includes("task") || identity.includes("kanban")) {
      return BriefcaseBusiness;
    }
    if (identity.includes("pin")) return Pin;
    if (identity.includes("new ") || identity.includes("create")) return Plus;
    return null;
  }

  function executionVerb(command: WorkshopCommand): string {
    if (command.prompt) return "Continue";
    if (command.id.startsWith("spotlight-tab:") || command.id.startsWith("spotlight-pane:")) {
      return "Focus";
    }
    const first = command.label.trim().split(/\s+/)[0]?.toLowerCase() ?? "";
    const verbs: Record<string, string> = {
      change: "Change",
      check: "Check",
      clear: "Clear",
      create: "Create",
      edit: "Edit",
      export: "Export",
      fetch: "Fetch",
      hide: "Hide",
      new: "Create",
      open: "Open",
      pin: "Pin",
      pull: "Pull",
      push: "Push",
      remove: "Remove",
      rename: "Rename",
      reset: "Reset",
      resume: "Resume",
      run: "Run",
      search: "Search",
      seal: "Seal",
      show: "Show",
      switch: "Switch",
      sync: "Sync",
      toggle: "Toggle",
      zoom: "Zoom",
    };
    return verbs[first] ?? (command.section === "go" || command.section === "open" ? "Open" : "Run");
  }

  function executionLabel(command: WorkshopCommand): string {
    const verb = executionVerb(command);
    const first = command.label.trim().split(/\s+/)[0]?.toLowerCase() ?? "";
    if (verb === "Create" && first === "new") {
      return `Create ${command.label.trim().slice(4)}`;
    }
    if (first === verb.toLowerCase()) return command.label;
    return `${verb} ${command.label}`;
  }

  function previewForTab(tab: ShellTab): CommandPreview | undefined {
    if (tab.kind === "chat") {
      const session = chat.sessions.find((entry) => entry.session_id === tab.sessionId);
      return {
        kind: "chat",
        sessionId: tab.sessionId,
        text: session?.preview?.trim() || `Open conversation “${tab.title}”.`,
      };
    }
    if (tab.kind === "lme") {
      const lme = lmeWorkspace.tabs.find((entry) => entry.tabId === tab.lmeTabId);
      if (lme?.kind === "note") return { kind: "note", path: lme.path };
    }
    return undefined;
  }

  function matchesWorkspaceQuery(command: WorkshopCommand, value: string): boolean {
    const needle = value.trim().toLowerCase();
    if (!needle) return true;
    return `${command.label} ${command.subtitle ?? ""} ${command.keywords ?? ""}`
      .toLowerCase()
      .includes(needle);
  }

  function collectDesktopGroups(
    desktopId: string,
    layout: ShellDesktopLayout,
    value: string,
  ): SpotlightGroup[] {
    const order = leafOrder(layout.splitRoot);
    const paneByTabId = new Map<string, number>();
    for (const [paneOffset, groupId] of order.entries()) {
      const group = layout.groups.find((entry) => entry.id === groupId);
      for (const tabId of group?.tabIds ?? []) paneByTabId.set(tabId, paneOffset + 1);
    }

    const tabs = layout.tabs
      .map((tab): WorkshopCommand => ({
        id: `spotlight-tab:${desktopId}:${tab.id}`,
        section: "open",
        label: tab.title,
        subtitle: `${tabKindLabel(tab)} · Pane ${paneByTabId.get(tab.id) ?? 1}`,
        keywords: `${tab.kind} ${tab.title}`,
        preview: previewForTab(tab),
        run: async (runCtx) => {
          await shellTabs.revealSearchHit(desktopId, tab.id);
          runCtx.callbacks.close();
        },
      }))
      .filter((command) => matchesWorkspaceQuery(command, value));

    const panes = order
      .map((groupId, paneOffset): WorkshopCommand | null => {
        const group = layout.groups.find((entry) => entry.id === groupId);
        if (!group) return null;
        const paneTabs = group.tabIds
          .map((id) => layout.tabs.find((tab) => tab.id === id))
          .filter((tab): tab is ShellTab => Boolean(tab));
        const active = paneTabs.find((tab) => tab.id === group.activeTabId) ?? paneTabs[0];
        const count = paneTabs.length;
        return {
          id: `spotlight-pane:${desktopId}:${groupId}`,
          section: "open",
          label: `Pane ${paneOffset + 1}${active ? ` · ${active.title}` : ""}`,
          subtitle: count === 0 ? "Empty tiled window" : `${count} open tab${count === 1 ? "" : "s"}`,
          keywords: `pane tile window ${paneTabs.map((tab) => tab.title).join(" ")}`,
          run: async (runCtx) => {
            if (desktopId !== shellTabs.activeDesktopId) {
              await shellTabs.switchDesktop(desktopId);
            }
            shellTabs.focusGroup(groupId);
            runCtx.callbacks.close();
          },
        };
      })
      .filter((command): command is WorkshopCommand => Boolean(command))
      .filter((command) => matchesWorkspaceQuery(command, value));

    return [
      ...(tabs.length > 0 ? [{ id: "open-tabs", label: "Open tabs", commands: tabs }] : []),
      ...(panes.length > 0
        ? [{ id: "tiled-windows", label: "Tiled windows", commands: panes }]
        : []),
    ];
  }

  /** Native browser embed draws over the DOM — hide it while spotlight is open. */
  $effect(() => {
    if (!commandSpotlight.open) return;
    void pushBrowserPopoverOverlay();
    return () => {
      void popBrowserPopoverOverlay();
    };
  });

  /** Side effects + command collection belong in $effect, never $derived. */
  $effect(() => {
    if (!commandSpotlight.open) {
      groups = [];
      return;
    }

    if (vault.notes.length === 0) {
      void vault.refreshNotes();
    }
    if (workshop.scripts.length === 0) {
      void workshop.refreshModulesAndScripts();
    }

    void vault.notes;
    void vault.labelByPathMap;
    void chat.sessions;
    void chat.pendingBudgetApprovals;
    void chat.contextUsage;
    void chat.liveStreamActive;
    void connection.offline;
    void workspace.cards;
    void workshop.scripts;
    void spotlightPins.slots;
    void shellTabs.desktops;
    void shellTabs.tabs;
    void shellTabs.groups;
    void shellTabs.splitRoot;
    void shellTabs.activeDesktopId;
    void lmeWorkspace.tabs;
    void commandSpotlight.mode;
    void selectedScopeId;
    void selectedLens;
    void query;
    void promptStep;
    void ctx;

    try {
      const layout = selectedDesktopLayout;
      if (selectedScopeId !== "home" && layout) {
        groups = collectDesktopGroups(selectedScopeId, layout, query);
      } else if (!notesMode && !query.trim()) {
        groups = collectWorkshopLensCommands(ctx, selectedLens).map((group) => ({
          id: `${selectedLens}-${group.label}`,
          label: group.label,
          commands: group.commands,
        }));
      } else {
        groups = collectWorkshopCommands(ctx, {
          query,
          notesMode,
        }).map((group) => ({
          id: group.section,
          label: group.label,
          commands: group.commands,
        }));
      }
    } catch (err) {
      console.error("Command spotlight failed to collect commands", err);
      groups = [];
    }
  });

  /** Reset / focus when Spotlight opens or enters a prompt — not on every keystroke. */
  $effect(() => {
    const isOpen = commandSpotlight.open;
    const step = promptStep;
    if (!isOpen) {
      query = "";
      selectedScopeId = "home";
      selectedLens = "overview";
      promptValue = "";
      busy = false;
      previewText = null;
      previewTitle = null;
      highlightIndex = 0;
      return;
    }

    // Intentionally do not read query/seed here — that re-ran this effect on every keystroke
    // and fought resume hydration.
    void step;
    highlightIndex = 0;

    const frame = requestAnimationFrame(() => {
      inputEl?.focus();
    });
    return () => cancelAnimationFrame(frame);
  });

  $effect(() => {
    if (selectedScopeId === "home") return;
    if (shellTabs.desktops.some((desktop) => desktop.id === selectedScopeId)) return;
    selectedScopeId = "home";
    highlightIndex = 0;
  });

  $effect(() => {
    if (!commandSpotlight.open) return;
    void selectedScopeId;
    highlightIndex = 0;
  });

  /** Hydrate query from resume / restore (open or already open). */
  $effect(() => {
    const seed = commandSpotlight.seedQuery;
    if (!commandSpotlight.open || seed == null || promptStep) return;
    query = seed;
    commandSpotlight.seedQuery = null;
    highlightIndex = 0;
  });

  $effect(() => {
    const len = flatCommands.length;
    if (highlightIndex >= len) {
      highlightIndex = Math.max(0, len - 1);
    }
  });

  /** Telescope-lite preview for highlighted row (keyed by command id). */
  $effect(() => {
    if (!commandSpotlight.open) {
      previewText = null;
      previewTitle = null;
      return;
    }
    const command = activeCommand;
    const commandId = command?.id ?? null;
    const preview = command?.preview;
    if (!commandId || !command) {
      previewText = null;
      previewTitle = null;
      return;
    }

    let cancelled = false;
    previewTitle = command.label;

    if (!preview) {
      previewText =
        command.subtitle?.trim() ||
        command.hint?.trim() ||
        "Focus this workshop object or press Enter to continue.";
      return;
    }

    if (preview.kind === "text") {
      previewText = preview.text;
      return;
    }
    if (preview.kind === "script") {
      previewText = preview.body?.trim() || "Loading script…";
      if (!preview.body) {
        void (async () => {
          try {
            const { getGraphemeScript } = await import("$lib/daemon");
            const detail = await getGraphemeScript(preview.scriptId);
            if (!cancelled) previewText = detail.body_preview || "(empty script)";
          } catch {
            if (!cancelled) previewText = "Couldn’t load script preview.";
          }
        })();
      }
      return () => {
        cancelled = true;
      };
    }
    if (preview.kind === "chat") {
      previewText = preview.text?.trim() || "Open this conversation.";
      return;
    }
    if (preview.kind === "note") {
      previewText = "Loading note…";
      void (async () => {
        try {
          const note = await getVaultNote(preview.path);
          if (cancelled) return;
          previewText = (note.content ?? "").slice(0, 4_000);
        } catch {
          if (!cancelled) previewText = "Couldn’t load note preview.";
        }
      })();
    }

    return () => {
      cancelled = true;
    };
  });

  async function runCommand(command: WorkshopCommand, args?: string) {
    if (busy) return;
    if (command.prompt && !args) {
      commandSpotlight.beginPrompt(
        {
          commandId: command.id,
          label: command.label,
          placeholder: command.prompt.placeholder,
          submitLabel: command.prompt.submitLabel ?? "Run",
        },
        command,
      );
      promptValue = "";
      requestAnimationFrame(() => inputEl?.focus());
      return;
    }
    busy = true;
    try {
      commandSpotlight.rememberQuery(query, commandSpotlight.mode);
      await executeWorkshopCommand(ctx, command, args);
    } catch (err) {
      ctx.error(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function submitPrompt() {
    const step = promptStep;
    const command = commandSpotlight.pendingCommand;
    if (!step || !command) {
      commandSpotlight.cancelPrompt();
      return;
    }
    const value = promptValue.trim();
    if (!value) return;
    commandSpotlight.cancelPrompt();
    await runCommand(command, value);
  }

  function selectScope(scopeId: string) {
    if (scopeId === selectedScopeId) return;
    highlightNavigation = "data";
    selectedScopeId = scopeId;
    highlightIndex = 0;
    requestAnimationFrame(() => inputEl?.focus());
  }

  function selectLens(lens: SpotlightLens) {
    if (lens === selectedLens) return;
    highlightNavigation = "data";
    selectedLens = lens;
    highlightIndex = 0;
    requestAnimationFrame(() => inputEl?.focus());
  }

  function createWorkspace() {
    const command = buildWorkspaceCommands().find((entry) => entry.id === "workspace-new");
    if (command) void runCommand(command);
  }

  function focusCommand(commandId: string) {
    const index = flatCommands.findIndex((command) => command.id === commandId);
    if (index >= 0) {
      highlightNavigation = "pointer";
      highlightIndex = index;
    }
  }

  function moveScope(delta: number) {
    const index = scopeTabs.findIndex((scope) => scope.id === selectedScopeId);
    const from = index < 0 ? 0 : index;
    const next = scopeTabs[(from + delta + scopeTabs.length) % scopeTabs.length];
    if (next) selectScope(next.id);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!commandSpotlight.open) return;

    if (event.key === "Escape") {
      event.preventDefault();
      if (promptStep) {
        commandSpotlight.cancelPrompt();
      } else {
        commandSpotlight.rememberQuery(query, commandSpotlight.mode);
        commandSpotlight.closeSpotlight();
      }
      return;
    }

    if (promptStep) {
      if (event.key === "Enter") {
        event.preventDefault();
        void submitPrompt();
      }
      return;
    }

    if (event.ctrlKey && !event.metaKey && !event.altKey && event.key === "ArrowLeft") {
      event.preventDefault();
      moveScope(-1);
      return;
    }
    if (event.ctrlKey && !event.metaKey && !event.altKey && event.key === "ArrowRight") {
      event.preventDefault();
      moveScope(1);
      return;
    }

    // Harpoon: digits 1–4 jump pins when query is empty.
    if (selectedScopeId === "home" && !query.trim() && /^[1-4]$/.test(event.key) && !event.metaKey && !event.ctrlKey && !event.altKey) {
      const slot = Number(event.key) - 1;
      if (jumpPinSlot(ctx, slot)) {
        event.preventDefault();
        return;
      }
    }

    if (flatCommands.length === 0) return;

    if (event.key === "ArrowDown") {
      event.preventDefault();
      highlightNavigation = "keyboard";
      highlightIndex = (highlightIndex + 1) % flatCommands.length;
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      highlightNavigation = "keyboard";
      highlightIndex = (highlightIndex - 1 + flatCommands.length) % flatCommands.length;
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const command = flatCommands[highlightIndex];
      if (command) void runCommand(command);
    }
  }

  function globalIndex(sectionIndex: number, itemIndex: number): number {
    let index = 0;
    for (let s = 0; s < sectionIndex; s += 1) {
      index += groups[s]?.commands.length ?? 0;
    }
    return index + itemIndex;
  }

  function queueScrollActiveRow(block: ScrollLogicalPosition) {
    requestAnimationFrame(() => {
      const row = resultsEl?.querySelector<HTMLElement>(
        `[data-spotlight-index="${highlightIndex}"]`,
      );
      row?.scrollIntoView({ block });
    });
  }

  $effect(() => {
    if (!commandSpotlight.open) return;
    void highlightIndex;
    void groups;
    queueScrollActiveRow(highlightNavigation === "keyboard" ? "center" : "nearest");
  });

  const prefixHint = $derived(parseSpotlightQuery(query).mode);
</script>

<svelte:window onkeydown={handleKeydown} />

{#if commandSpotlight.open}
  <div
    class="command-spotlight-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) {
        commandSpotlight.rememberQuery(query, commandSpotlight.mode);
        commandSpotlight.closeSpotlight();
      }
    }}
  >
    <div
      class="command-spotlight-panel"
      class:command-spotlight-panel-contextual={showStage && !hasRichStage}
      class:command-spotlight-panel-rich={showStage && hasRichStage}
      role="dialog"
      aria-modal="true"
      aria-label="Command spotlight"
    >
      {#if promptStep}
        <div class="command-spotlight-prompt-header">
          <p class="command-spotlight-kicker">Follow-up</p>
          <p class="command-spotlight-prompt-label">{promptStep.label}</p>
        </div>
        <input
          bind:this={inputEl}
          class="command-spotlight-input"
          placeholder={promptStep.placeholder}
          bind:value={promptValue}
          disabled={busy}
        />
      {:else}
        <input
          bind:this={inputEl}
          class="command-spotlight-input"
          {placeholder}
          bind:value={query}
          oninput={() => {
            highlightNavigation = "data";
            highlightIndex = 0;
          }}
          disabled={busy}
        />
        {#if prefixHint !== "default"}
          <p class="command-spotlight-mode-chip">
            {#if prefixHint === "create"}
              Create
            {:else if prefixHint === "run"}
              Run
            {:else}
              Advanced
            {/if}
          </p>
        {/if}
      {/if}

      {#if !promptStep}
        <div class="command-spotlight-navigation">
          <nav class="command-spotlight-scopes" aria-label="Workshop scopes">
            <span class="command-spotlight-scopes-label">Search in</span>
            <div class="command-spotlight-scope-list" role="tablist" aria-label="Workspaces">
              {#each scopeTabs as scope (scope.id)}
                <button
                  type="button"
                  role="tab"
                  class="command-spotlight-scope"
                  class:command-spotlight-scope-active={scope.id === selectedScopeId}
                  aria-selected={scope.id === selectedScopeId}
                  onclick={() => selectScope(scope.id)}
                >
                  <span class="command-spotlight-scope-content">
                    {#if scope.id === "home"}
                      <House size={12} strokeWidth={1.7} />
                    {/if}
                    <span>{scope.label}</span>
                  </span>
                </button>
              {/each}
              <button
                type="button"
                class="command-spotlight-workspace-add"
                aria-label="Create workspace"
                title="Create workspace"
                onclick={createWorkspace}
              >
                <Plus size={13} strokeWidth={1.7} />
              </button>
            </div>
          </nav>

          {#if selectedScopeId === "home" && !notesMode}
            {#if query.trim()}
              <span class="command-spotlight-searching-all">All results</span>
            {:else}
              <nav class="command-spotlight-lenses" aria-label="Spotlight categories">
                {#each lensTabs as lens (lens.id)}
                  <button
                    type="button"
                    class="command-spotlight-lens"
                    class:command-spotlight-lens-active={lens.id === selectedLens}
                    aria-current={lens.id === selectedLens ? "page" : undefined}
                    onclick={() => selectLens(lens.id)}
                  >
                    <span class="command-spotlight-lens-content">{lens.label}</span>
                  </button>
                {/each}
              </nav>
            {/if}
          {/if}
        </div>
      {/if}

      <div
        class="command-spotlight-body"
        class:command-spotlight-body-rich={hasRichStage}
      >
        <div class="command-spotlight-results" bind:this={resultsEl}>
          {#each groups as group, sectionIndex (group.id)}
            <div class="command-spotlight-section-label">{group.label}</div>
            <ul class="command-spotlight-list">
              {#each group.commands as command, itemIndex (command.id)}
                {@const rowIndex = globalIndex(sectionIndex, itemIndex)}
                {@const CommandIcon = iconForCommand(command)}
                <li>
                  <button
                    type="button"
                    class="command-spotlight-row"
                    class:command-spotlight-row-active={rowIndex === highlightIndex}
                    data-spotlight-index={rowIndex}
                    disabled={busy}
                    onmousemove={() => {
                      highlightNavigation = "pointer";
                      highlightIndex = rowIndex;
                    }}
                    onclick={() => void runCommand(command)}
                  >
                    <span class="command-spotlight-row-icon" aria-hidden="true">
                      {#if CommandIcon}
                        <CommandIcon size={14} strokeWidth={1.6} />
                      {/if}
                    </span>
                    <span class="command-spotlight-row-copy">
                      <span class="command-spotlight-row-title">{command.label}</span>
                      {#if command.subtitle}
                        <span class="command-spotlight-row-subtitle">{command.subtitle}</span>
                      {/if}
                    </span>
                    <span class="command-spotlight-row-meta">
                      {#if command.risk === "attention"}
                        <span class="command-spotlight-attention">Needs attention</span>
                      {/if}
                      {#if rowIndex === highlightIndex}
                        <span class="command-spotlight-row-run">
                          <span>↵</span> {executionVerb(command)}
                        </span>
                      {:else if command.hint}
                        <span class="command-spotlight-hint">{command.hint}</span>
                      {/if}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="command-spotlight-empty">No matching commands</p>
          {/each}
        </div>

        {#if activeCommand && showStage}
          {@const ActiveIcon = iconForCommand(activeCommand)}
          <aside class="command-spotlight-preview" aria-label="Preview">
            <header class="command-spotlight-stage-header">
              <div class="command-spotlight-stage-kicker">
                {#if ActiveIcon}
                  <ActiveIcon size={13} strokeWidth={1.6} />
                {/if}
                <span>{activeGroup?.label ?? "Workshop"}</span>
              </div>
              <h2>{activeCommand.label}</h2>
              {#if activeCommand.subtitle}
                <p>{activeCommand.subtitle}</p>
              {/if}
            </header>

            {#if activePreviewKind === "chat" && chatPreviewCommands.length > 0}
              <div class="command-spotlight-preview-tabs" role="tablist" aria-label="Recent chats">
                {#each chatPreviewCommands as command (command.id)}
                  <button
                    type="button"
                    role="tab"
                    aria-selected={command.id === activeCommand.id}
                    class="command-spotlight-preview-tab"
                    class:command-spotlight-preview-tab-active={command.id === activeCommand.id}
                    onclick={() => focusCommand(command.id)}
                  >
                    {command.label.replace(/^Open chat:\s*/, "")}
                  </button>
                {/each}
              </div>
            {/if}

            <div class="command-spotlight-stage-content">
              {#if selectedDesktopLayout && activePreviewKind === "fallback"}
                <div class="command-spotlight-workspace-preview">
                  <SpotlightWorkspacePreview
                    layout={selectedDesktopLayout}
                    selectedTabId={activeWorkspaceTabId}
                  />
                </div>
              {:else if activePreviewKind === "note" && previewText}
                <div class="command-spotlight-preview-markdown">
                  <MarkdownContent content={previewText} titleByPath={vault.labelByPathMap} />
                </div>
              {:else if activePreviewKind === "chat"}
                <div class="command-spotlight-chat-preview">
                  <div class="command-spotlight-chat-meta">
                    <MessageSquare size={13} strokeWidth={1.6} />
                    <span>Recent conversation</span>
                  </div>
                  <p>{previewText}</p>
                </div>
              {:else if activePreviewKind !== "fallback"}
                <pre class="command-spotlight-preview-body">{previewText}</pre>
              {:else if activeCommand.risk === "attention"}
                <div class="command-spotlight-stage-warning">
                  <span>Needs attention</span>
                  <p>Review this action before running it in the current workspace.</p>
                </div>
              {/if}
            </div>

            <div class="command-spotlight-stage-action">
              <span class="command-spotlight-stage-return">↵</span>
              <span>{executionLabel(activeCommand)}</span>
            </div>
          </aside>
        {/if}
      </div>

      <footer class="command-spotlight-footer">
        <span>↑↓ Navigate · ↵ Run · esc Close</span>
        <span class="command-spotlight-kbd">{formatShortcut("K")}</span>
      </footer>
    </div>
  </div>
{/if}

{#if sessionExportPreview.open}
  {#await loadVaultExportPreviewModal() then { default: VaultExportPreviewModal }}
    <VaultExportPreviewModal
      open={sessionExportPreview.open}
      title={sessionExportPreview.title}
      content={sessionExportPreview.content}
      labelByPath={new Map()}
      notePath={null}
      initialFormat="pdf"
      onClose={() => sessionExportPreview.close()}
    />
  {/await}
{/if}
