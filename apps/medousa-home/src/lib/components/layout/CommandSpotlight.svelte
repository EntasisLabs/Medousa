<script lang="ts">
  import { onMount, untrack, type Component } from "svelte";
  import { createSpotlightConversation, openAddressedConversation } from "$lib/spotlight/spotlightConversation.svelte";
  import { mentionQuery } from "$lib/spotlight/conversationTargets";
  import SpotlightAddressInput from "./SpotlightAddressInput.svelte";
  import { guardTextNavigation } from "$lib/utils/guardTextNavigation";
  import { textNavigationKey } from "$lib/utils/composerKeyboard";
  import { vaultSearchCatalog } from "$lib/vault/vaultSearchCatalog.svelte";
  import SpotlightTranscript from "./SpotlightTranscript.svelte";
  import BotAvatar from "$lib/components/chat/BotAvatar.svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { connectedAgents } from "$lib/stores/connectedAgents.svelte";
  import "$lib/styles/command-spotlight.postcss";
  import { buildWorkshopCommandContext } from "$lib/commands/context";
  import {
    collectWorkshopCommands,
    collectWorkshopLensCommands,
    parseSpotlightQuery,
    type SpotlightLens,
  } from "$lib/commands/collectCommands";
  import { jumpPinSlot } from "$lib/commands/pinCommands";
  import { executeWorkshopCommand } from "$lib/commands/runWorkshopCommand";
  import { buildWorkspaceCommands } from "$lib/commands/registry";
  import { buildSshTerminalCommands } from "$lib/commands/terminalCommands";
  import { filterAndSortCommands } from "$lib/commands/score";
  import { SpotlightSshTargets } from "$lib/spotlight/sshTargets.svelte";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
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
  import type { WorkshopCommand } from "$lib/commands/types";
  import type { ShellDesktopLayout } from "$lib/types/shellTabs";
  import {
    popBrowserPopoverOverlay,
    pushBrowserPopoverOverlay,
  } from "$lib/utils/browserPopoverOverlay";
  import { isTauriDesktop } from "$lib/platform";
  import { loadVaultExportPreviewModal } from "$lib/runtime/viewLoaders";
  import { collectDesktopGroups, spotlightTabKind as tabKindLabel, spotlightTabPreview as previewForTab } from "$lib/spotlight/desktopGroups";
  import type { SpotlightHost, SpotlightSnapshot } from "$lib/spotlight/host";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import SpotlightWorkspacePreview from "./SpotlightWorkspacePreview.svelte";
  import SpotlightActivity from "./SpotlightActivity.svelte";
  import SpotlightNavigation from "./SpotlightNavigation.svelte";
  import SpotlightTerminalStatus from "./SpotlightTerminalStatus.svelte";
  import {
    Bot,
    Boxes,
    BriefcaseBusiness,
    FileText,
    FlaskConical,
    FolderOpen,
    GitBranch,
    Globe2,
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

  interface Props { onFocusChat?: () => void; additionalCommands?: WorkshopCommand[]; host?: SpotlightHost; onDismiss?: () => void; }

  interface SpotlightGroup {
    id: string;
    label: string;
    commands: WorkshopCommand[];
  }

  let { onFocusChat, additionalCommands = [], host, onDismiss }: Props = $props();
  let snapshot = $state<SpotlightSnapshot | null>(null);
  let snapshotScope = $state("home");
  let refreshRevision = $state(0);
  let collecting = $state(false);
  let lastRequestKey = "";
  let lastHost: SpotlightHost | undefined;
  let mounted = false;

  let query = $state("");
  const conversation = createSpotlightConversation(() => Boolean(mention), () => { query = ""; }, () => Boolean(host));
  let addressInput = $state<{ handleKeydown: (event: KeyboardEvent) => boolean }>();
  const mention = $derived(commandSpotlight.mode === "default" && !commandSpotlight.promptStep && !conversation.target ? mentionQuery(query) : null);
  const addressing = $derived(Boolean(conversation.target) || Boolean(mention));
  function removeTarget() { conversation.clear(); query = ""; }
  let highlightIndex = $state(0);
  let busy = $state(false);
  let commandError = $state<string | null>(null);
  let sshData = $state<SpotlightSshTargets | null>(null);
  $effect(() => {
    if (!commandSpotlight.open || !isTauriDesktop()) return;
    const scope = activeWorkshopId();
    const data = new SpotlightSshTargets(scope);
    untrack(() => { sshData = data; void data.refresh(); });
    return () => { data.dispose(); sshData = null; };
  });
  let inputEl = $state<HTMLInputElement | null>(null);
  let promptValue = $state("");
  let selectedScopeId = $state("home");
  let selectedLens = $state<SpotlightLens>(commandSpotlight.activityFilter ? "activity" : "overview");
  $effect(() => {
    if (commandSpotlight.activityFilter) { selectedLens = "activity"; selectedScopeId = "home"; query = ""; }
  });
  let groups = $state<SpotlightGroup[]>([]);
  let previewText = $state<string | null>(null);
  let previewTitle = $state<string | null>(null);
  let resultsEl = $state<HTMLDivElement | null>(null);
  let highlightNavigation = $state<"keyboard" | "pointer" | "data">("data");
  $effect(() => {
    const scope = chat.workshopScopeId;
    if (commandSpotlight.open && scope && !connection.offline) untrack(() => {void bots.refresh().catch(() => undefined);void connectedAgents.refresh(scope);});
  });
  onMount(() => {
    mounted = true;
    const timer = window.setInterval(() => { if (host && commandSpotlight.open && !busy && !promptStep && document.visibilityState === "visible") refreshRevision += 1; }, 5000);
    const changed = () => {if (chat.workshopScopeId) void connectedAgents.refresh(chat.workshopScopeId,true);};
    window.addEventListener("medousa-external-conversation-changed",changed);
    return () => { mounted = false; clearInterval(timer); window.removeEventListener("medousa-external-conversation-changed",changed); };
  });
  const notesMode = $derived(commandSpotlight.mode === "notes");
  const sshMode = $derived(commandSpotlight.mode === "ssh");
  const activityMode = $derived(!addressing && !notesMode && !sshMode && selectedScopeId === "home" && selectedLens === "activity");
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

  $effect(() => { if (!commandSpotlight.open) onDismiss?.(); });

  const flatCommands = $derived(groups.flatMap((group) => group.commands));
  const activeCommand = $derived(flatCommands[highlightIndex] ?? null);
  const activePreviewKind = $derived(activeCommand?.preview?.kind ?? "fallback");
  const activeGroup = $derived(
    activeCommand
      ? groups.find((group) => group.commands.some((command) => command.id === activeCommand.id)) ??
          null
      : null,
  );

  const scopeTabs = $derived(host && snapshot ? snapshot.scopes : [
    { id: "home", label: "Home" },
    ...shellTabs.desktops.map((desktop) => ({ id: desktop.id, label: desktop.name })),
  ]);

  const lensTabs: Array<{ id: SpotlightLens; label: string }> = [
    { id: "overview", label: "Overview" },
    { id: "recent", label: "Recent" },
    { id: "activity", label: "Activity" },
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
    if (host) return snapshotScope === selectedScopeId ? snapshot?.layout ?? null : null;
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
    sshMode ? "Search saved SSH servers…" : notesMode
      ? "Search notes…"
      : activityMode
        ? "Search activity…"
      : selectedScopeId === "home"
        ? "Search this workshop…"
        : `Search ${scopeTabs.find(scope => scope.id === selectedScopeId)?.label ?? "workspace"}…`,
  );

  function iconForCommand(command: WorkshopCommand): Component | null {
    const identity = `${command.id} ${command.label} ${command.keywords ?? ""}`.toLowerCase();
    if (command.id.startsWith("spotlight-pane:")) return LayoutPanelTop;
    if (command.preview?.kind === "agent") return Bot;
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
    if (command.preview?.kind === "agent") return "Open";
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
      connect: "Connect",
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

  /** Native browser embed draws over the DOM — hide it while spotlight is open. */
  $effect(() => {
    if (host || !commandSpotlight.open) return;
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

    if (addressing) { groups = []; collecting = false; return; }
    if (host && !sshMode) {
      const owner = host;
      const request = { query, mode: commandSpotlight.mode, lens: selectedLens, scopeId: selectedScopeId };
      const extras = additionalCommands;
      void refreshRevision;
      let cancelled = false;
      const key = JSON.stringify(request);
      const changed = lastHost !== owner || lastRequestKey !== key;
      lastHost = owner; lastRequestKey = key;
      collecting = changed || untrack(() => groups.length === 0);
      if (changed) groups = [];
      const timer = window.setTimeout(() => {
        void owner.collect(request).then(value => {
          if (cancelled) return;
          const selectedId = changed ? null : untrack(() => flatCommands[highlightIndex]?.id);
          snapshot = value; snapshotScope = request.scopeId; commandError = null;
          groups = value.groups.map(group => ({ ...group, commands: group.commands.map(command => ({ ...command, run: async (_ctx, args) => { await owner.execute(command, args); } })) }));
          if (request.scopeId === "home" && parseSpotlightQuery(request.query).mode === "default" && request.lens !== "activity") {
            const ids = new Set(groups.flatMap(group => group.commands.map(command => command.id)));
            const sessions = new Set(groups.flatMap(group => group.commands.flatMap(command => command.preview?.kind === "chat" ? [command.preview.sessionId] : [])));
            const extra = filterAndSortCommands(extras.filter(command => !ids.has(command.id) && !(command.preview?.kind === "chat" && sessions.has(command.preview.sessionId))), request.query, 32);
            if (extra.length) groups.push({ id: "resources", label: "Open a window", commands: extra });
          }
          if (selectedId) { const index = groups.flatMap(group => group.commands).findIndex(command => command.id === selectedId); if (index >= 0) highlightIndex = index; }
        }).catch(() => {
          if (!cancelled) {
            groups = [{ id: "resources", label: "Open a window", commands: filterAndSortCommands(extras, request.query, 32) }];
            if (!groups[0].commands.length) commandError = "Couldn’t search this workshop. Try again.";
          }
        }).finally(() => { if (!cancelled) collecting = false; });
      }, request.query ? 80 : 0);
      return () => { cancelled = true; clearTimeout(timer); };
    }

    if (notesMode || ["default", "advanced"].includes(parseSpotlightQuery(query).mode)) void vaultSearchCatalog.search(parseSpotlightQuery(query).rawQuery);
    if (vault.notes.length === 0) {
      void vault.refreshNotes();
    }
    if (workshop.scripts.length === 0) {
      void workshop.refreshModulesAndScripts();
    }

    void vault.notes;
    void vault.labelByPathMap;
    void bots.bots;
    void connectedAgents.conversations;
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
    void sshMode;
    void sshData?.targets;

    try {
      const layout = selectedDesktopLayout;
      if (addressing) {
        groups = [];
      } else if (sshMode) {
        const commands = buildSshTerminalCommands(sshData?.targets ?? [], sshData?.scope ?? "");
        groups = [{ id: "ssh-servers", label: "Saved SSH servers", commands: filterAndSortCommands(host ? commands.map(command => ({ ...command, run: async () => { await host!.openSsh(command.id.slice("terminal-ssh:".length)); } })) : commands, query, 64) }];
      } else if (selectedScopeId !== "home" && layout) {
        groups = collectDesktopGroups(selectedScopeId, layout, query, previewForTab, tabKindLabel);
      } else if (activityMode) {
        groups = [];
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
          sshTargets: sshData?.targets ?? [],
          workshopScope: sshData?.scope,
        }).map((group) => ({
          id: `${group.section}-${group.label}`,
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
    const mode = commandSpotlight.mode;
    if (!isOpen) {
      conversation.clear();
      query = "";
      selectedScopeId = "home";
      selectedLens = "overview";
      promptValue = "";
      busy = false;
      commandError = null;
      previewText = null;
      previewTitle = null;
      highlightIndex = 0;
      return;
    }

    // Intentionally do not read query/seed here — that re-ran this effect on every keystroke
    // and fought resume hydration.
    void step;
    void mode;
    highlightIndex = 0;

    const frame = requestAnimationFrame(() => {
      inputEl?.focus();
    });
    return () => cancelAnimationFrame(frame);
  });

  $effect(() => {
    if (selectedScopeId === "home") return;
    if (scopeTabs.some(scope => scope.id === selectedScopeId)) return;
    if (host && !snapshot) return;
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

    if (preview.kind === "agent") {previewText = preview.description;return;}
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
    const executionHost = host;
    commandError = null;
    try {
      if (command.id !== "do-resume-spotlight") commandSpotlight.rememberQuery(query, commandSpotlight.mode);
      await executeWorkshopCommand(ctx, command, args);
    } catch (err) {
      if (executionHost && (!mounted || executionHost !== host || !commandSpotlight.open)) return;
      commandError = err instanceof Error ? err.message : String(err);
      ctx.error(commandError);
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

  export function dismiss() { commandSpotlight.rememberQuery(query, commandSpotlight.mode); commandSpotlight.closeSpotlight(); }

  function reportHostError(cause: unknown) { if (mounted && commandSpotlight.open) commandError = cause instanceof Error ? cause.message : String(cause); }

  function createWorkspace() {
    if (host) {
      const owner = host;
      void owner.collect({ query: "New workspace", mode: "default", lens: "actions", scopeId: "home" }).then(value => {
        if (!mounted || !commandSpotlight.open || owner !== host) return;
        const command = value.groups.flatMap(group => group.commands).find(command => command.id === "workspace-new");
        if (command) void runCommand({ ...command, run: async (_ctx, args) => { await owner.execute(command, args); } });
      }).catch(reportHostError);
      return;
    }
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
    if (!commandSpotlight.open || event.isComposing) return;
    const navigationKey = textNavigationKey(event);

    if (host && (event.metaKey || event.ctrlKey) && !event.altKey) {
      const key = event.key.toLowerCase();
      if (key === "k" && !event.shiftKey) { event.preventDefault(); dismiss(); return; }
      if (key === "p" && event.shiftKey) { event.preventDefault(); commandSpotlight.openCommandPalette(); return; }
      if (key === "o" && !event.shiftKey) { event.preventDefault(); commandSpotlight.openNotes(); return; }
    }

    if (!notesMode && !sshMode && !promptStep && addressInput?.handleKeydown(event)) return;

    if (event.key === "Escape") {
      event.preventDefault();
      if (promptStep) {
        commandSpotlight.cancelPrompt();
      } else if (sshMode) {
        commandSpotlight.leaveSshTargets();
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

    if (event.ctrlKey && !event.metaKey && !event.altKey && navigationKey === "ArrowLeft") {
      event.preventDefault();
      moveScope(-1);
      return;
    }
    if (event.ctrlKey && !event.metaKey && !event.altKey && navigationKey === "ArrowRight") {
      event.preventDefault();
      moveScope(1);
      return;
    }

    // The Activity tree owns arrow navigation and opening its selected node.
    if (activityMode) return;

    // Digits typed in search are query text (including IP addresses). Pins use result focus.
    if (!sshMode && event.target !== inputEl && selectedScopeId === "home" && !query.trim() && /^[1-4]$/.test(event.key) && !event.metaKey && !event.ctrlKey && !event.altKey) {
      const slot = Number(event.key) - 1;
      if (host) { event.preventDefault(); void host.pin(slot).catch(reportHostError); return; }
      if (jumpPinSlot(ctx, slot)) {
        event.preventDefault();
        return;
      }
    }

    if (flatCommands.length === 0) return;

    if (navigationKey === "ArrowDown") {
      event.preventDefault();
      highlightNavigation = "keyboard";
      highlightIndex = (highlightIndex + 1) % flatCommands.length;
      return;
    }
    if (navigationKey === "ArrowUp") {
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
    class:command-spotlight-desktop={Boolean(host)}
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
      role="dialog"
      aria-modal="true"
      aria-label="Command spotlight"
    >
      {#if sshMode}
        <div class="command-spotlight-prompt-header command-spotlight-terminal-header">
          <button type="button" class="btn btn-sm variant-ghost-surface" disabled={busy} onclick={() => { commandSpotlight.leaveSshTargets(); requestAnimationFrame(() => inputEl?.focus()); }}>← Back</button>
          <p class="command-spotlight-prompt-label">Open SSH terminal</p>
        </div>
      {/if}
      {#if promptStep}
        <div class="command-spotlight-prompt-header">
          <p class="command-spotlight-kicker">Follow-up</p>
          <p class="command-spotlight-prompt-label">{promptStep.label}</p>
        </div>
        <input
          use:guardTextNavigation
          bind:this={inputEl}
          class="command-spotlight-input"
          placeholder={promptStep.placeholder}
          bind:value={promptValue}
          disabled={busy}
        />
      {:else}
        <SpotlightAddressInput bind:this={addressInput} bind:inputEl bind:query {conversation} {placeholder} {busy} enabled={!notesMode && !sshMode} onQueryChange={() => { highlightNavigation = "data"; highlightIndex = 0; }}/>
        {#if prefixHint !== "default" && !addressing && !activityMode && !sshMode}
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

      {#if !promptStep && !sshMode}
        <SpotlightNavigation {scopeTabs} {selectedScopeId} {selectedLens} {lensTabs} {query} {notesMode} {activityMode} conversationLabel={addressing ? "Conversation" : null} selectScope={(id) => { if (addressing) removeTarget(); selectScope(id); }} {selectLens} {createWorkspace} />
      {/if}

      {#if conversation.target}
        <SpotlightTranscript {conversation} onOpen={() => { if (host && conversation.sessionId) return host.openConversation(conversation.sessionId); openAddressedConversation(conversation); }}/>
      {:else if mention}
        <div class="command-spotlight-conversation"><p class="command-spotlight-conversation-empty">Choose who to talk to. Continue a conversation or start something new.</p></div>
      {:else if activityMode}
        {#key chat.workshopScopeId}
          <SpotlightActivity {query} {ctx} {host} />
        {/key}
      {:else}
      <div class="command-spotlight-body">
        <div class="command-spotlight-results" bind:this={resultsEl} aria-busy={collecting}>
          {#if collecting}<p class="command-spotlight-empty" role="status">Searching your workshop…</p>{/if}
          <SpotlightTerminalStatus {sshMode} data={sshData} {query} {commandError} empty={!flatCommands.length} onManage={() => { if (host) void host.manageConnection().catch(reportHostError); else { ctx.openSettingsSection("basement"); ctx.callbacks.close(); } }} />
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
                      {#if command.preview?.kind === "agent" && command.preview.avatarRef}
                        <BotAvatar reference={command.preview.avatarRef} size={22} />
                      {:else if CommandIcon}
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
            {#if !sshMode && !collecting}<p class="command-spotlight-empty">No matching commands</p>{/if}
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
              {:else if activeCommand?.preview?.kind === "agent"}
                <div class="p-5 flex flex-col items-center gap-3 text-center">
                  {#if activeCommand.preview.avatarRef}<BotAvatar reference={activeCommand.preview.avatarRef} size={64}/>{:else}<Bot size={32}/>{/if}
                  <strong>{activeCommand.preview.name}</strong><p class="text-sm text-content-secondary">{activeCommand.preview.description}</p>
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
      {/if}

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
