import {
  approveTurnBudgetRequest,
  checkDaemonHealth,
  denyTurnBudgetRequest,
  getSessionHistory,
  listManuscripts,
  listTurnBudgetRequests,
} from "$lib/daemon";
import {
  resetContentZoom,
  stepContentZoom,
} from "$lib/config/contentZoom";
import { homeChannelSurface, formatShortcut } from "$lib/platform";
import { onThisHostPhrase } from "$lib/platformCopy";
import { appUpdate } from "$lib/stores/appUpdate.svelte";
import { openAppUpdateDownload } from "$lib/utils/appUpdate";
import { openGuide } from "$lib/guide/openGuide";
import { isTauri, toggleDesktopToolbar } from "$lib/window";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { copyBrowserUrl, openUrlInDefaultBrowser } from "$lib/utils/browserActions";
import {
  dispatchBrowserFocusUrl,
  dispatchBrowserOpenBookmarks,
} from "$lib/utils/browserChromeEvents";
import { summonViewToolbar } from "$lib/utils/railPopoverSummon";
import {
  isMouseShakeToolbarEnabled,
  toggleMouseShakeToolbarEnabled,
} from "$lib/utils/mouseShake";
import { reconnectWorkshop } from "$lib/workshopConnection";
import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
import { createTurnTicket } from "$lib/daemon";
import { connection } from "$lib/stores/connection.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { toast } from "$lib/stores/toast.svelte";
import { sessionExportPreview } from "$lib/stores/sessionExportPreview.svelte";
import {
  downloadTextFile,
  sessionExportBasename,
  sessionTranscriptMarkdown,
} from "$lib/utils/sessionTranscript";
import type { Surface } from "$lib/types/ui";
import type { DepthMode } from "$lib/types/runtime";
import type { WorkshopCommand, WorkshopCommandContext } from "./types";

const GO_DESTINATIONS: { surface: Surface; label: string; subtitle: string; keywords: string }[] = [
  { surface: "chat", label: "Chat", subtitle: "Talk with Medousa", keywords: "message compose conversation" },
  { surface: "notes", label: "Notes", subtitle: "Vault notes and markdown", keywords: "vault notes documents library" },
  { surface: "files", label: "Files", subtitle: "Pinned folders on this machine", keywords: "files folders pin local" },
  { surface: "artifacts", label: "Artifacts", subtitle: "HTML decks and agent widgets", keywords: "artifacts presentations decks widgets" },
  { surface: "work", label: "Work", subtitle: "Tasks and kanban board", keywords: "kanban cards jobs" },
  { surface: "code", label: "Code", subtitle: "Projects, editing, understanding, and review", keywords: "code project repository editor forge test review source" },
  { surface: "web", label: "Browser", subtitle: "Built-in web workshop", keywords: "browser web surf" },
  { surface: "automations", label: "Automations", subtitle: "Scripts and schedules", keywords: "cron scripts grapheme" },
  { surface: "workshop", label: "Agents", subtitle: "Specialist agents in Workspace", keywords: "skills manuscripts workshop capabilities specialist agents" },
  { surface: "map", label: "Map", subtitle: "Sessions, moments, and notes", keywords: "memory locus context map moments" },
  { surface: "peers", label: "Peers", subtitle: "Nearby workshops and inbox", keywords: "peers nearby share trust lan inbox" },
  { surface: "profiles", label: "Profiles", subtitle: "People and identity", keywords: "profiles identity people" },
  { surface: "runtime", label: "Engine status", subtitle: "Jobs, delivery, health", keywords: "runtime engine daemon health" },
  { surface: "settings", label: "Settings", subtitle: "Preferences and connection", keywords: "settings preferences config" },
];

export function buildGoCommands(): WorkshopCommand[] {
  const destinations = GO_DESTINATIONS.map((dest) => ({
    id: `go-${dest.surface}`,
    section: "go" as const,
    label: dest.label,
    subtitle: dest.subtitle,
    keywords: dest.keywords,
    run: (ctx: WorkshopCommandContext) => {
      ctx.navigate(dest.surface);
      ctx.callbacks.close();
    },
  }));
  return [
    ...destinations,
    {
      id: "go-channels",
      section: "go",
      label: "Channels",
      subtitle: "Telegram, Discord, Slack — Settings → Sharing",
      keywords: "channels messaging telegram discord slack whatsapp sharing network",
      run: (ctx) => {
        ctx.openSettingsSection("network");
        ctx.callbacks.close();
      },
    },
    {
      id: "go-mcp-connections",
      section: "go",
      label: "MCP connections",
      subtitle: "Manage MCP servers in Settings → MCP",
      keywords: "mcp connections gateway servers tools",
      run: (ctx) => {
        ctx.openSettingsSection("mcp");
        ctx.callbacks.close();
      },
    },
  ];
}

export function buildWorkspaceCommands(): WorkshopCommand[] {
  const switchCommands = shellTabs.desktops.map((desktop, index) => ({
    id: `workspace-switch-${desktop.id}`,
    section: "go" as const,
    label: `Switch workspace: ${desktop.name}`,
    subtitle:
      desktop.id === shellTabs.activeDesktopId
        ? `Current · ${formatShortcut(`Ctrl+; ${index + 1}`)}`
        : `${formatShortcut(`Ctrl+; ${index + 1}`)} — layout only; vault and chat stay shared`,
    keywords: `workspace desktop virtual switch ${desktop.name} ${index + 1}`,
    run: async (ctx: WorkshopCommandContext) => {
      await shellTabs.switchDesktop(desktop.id);
      ctx.callbacks.close();
      ctx.notice(`Workspace: ${desktop.name}`);
    },
  }));

  const atCap = !shellTabs.canCreateDesktop;

  return [
    {
      id: "workspace-new",
      section: "go",
      label: "New workspace",
      subtitle: atCap
        ? "Maximum 4 virtual desktops"
        : "Create a named virtual desktop (pane layout snapshot)",
      keywords: "workspace desktop virtual new create hyprland",
      prompt: atCap
        ? undefined
        : {
            placeholder: "Workspace name",
            submitLabel: "Create",
          },
      run: (ctx, args) => {
        if (!shellTabs.canCreateDesktop) {
          ctx.error("You already have 4 workspaces.");
          return;
        }
        const id = shellTabs.createDesktop(args);
        if (!id) {
          ctx.error("You already have 4 workspaces.");
          return;
        }
        const name =
          shellTabs.desktops.find((desktop) => desktop.id === id)?.name ?? "Workspace";
        ctx.callbacks.close();
        ctx.notice(`Created workspace: ${name}`);
      },
    },
    {
      id: "workspace-rename",
      section: "go",
      label: "Rename workspace",
      subtitle: `Rename “${shellTabs.activeDesktopName}”`,
      keywords: "workspace desktop virtual rename",
      prompt: {
        placeholder: "New workspace name",
        submitLabel: "Rename",
      },
      run: (ctx, args) => {
        const ok = shellTabs.renameDesktop(shellTabs.activeDesktopId, args ?? "");
        if (!ok) {
          ctx.error("Enter a workspace name.");
          return;
        }
        ctx.callbacks.close();
        ctx.notice(`Renamed workspace: ${shellTabs.activeDesktopName}`);
      },
    },
    {
      id: "workspace-remove",
      section: "go",
      label: "Remove workspace",
      subtitle:
        shellTabs.desktops.length <= 1
          ? "Keep at least one workspace"
          : `Delete “${shellTabs.activeDesktopName}” (layout only)`,
      keywords: "workspace desktop virtual remove delete close",
      risk: shellTabs.desktops.length > 1 ? "attention" : "safe",
      run: async (ctx) => {
        if (shellTabs.desktops.length <= 1) {
          ctx.error("Keep at least one workspace.");
          return;
        }
        const name = shellTabs.activeDesktopName;
        const ok = await shellTabs.removeDesktop();
        if (!ok) {
          ctx.error("Could not remove workspace.");
          return;
        }
        ctx.callbacks.close();
        ctx.notice(`Removed workspace: ${name}`);
      },
    },
    ...switchCommands,
  ];
}

export function buildPaneCommands(): WorkshopCommand[] {
  return [
    {
      id: "pane-split-right",
      section: "advanced",
      label: "Split pane right",
      subtitle: `${formatShortcut("Ctrl+; %")} — TMUX-style vertical split`,
      keywords: "split pane right vertical tmux editor group window",
      advanced: true,
      run: (ctx) => {
        shellTabs.splitActive("right");
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-split-down",
      section: "advanced",
      label: "Split pane down",
      subtitle: `${formatShortcut('Ctrl+; "')} — TMUX-style horizontal split`,
      keywords: "split pane down horizontal tmux editor group window",
      advanced: true,
      run: (ctx) => {
        shellTabs.splitActive("down");
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-focus-left",
      section: "advanced",
      label: "Focus pane left",
      subtitle: `${formatShortcut("Ctrl+; h")} — move focus left`,
      keywords: "focus pane left split hjkl window",
      advanced: true,
      run: (ctx) => {
        shellTabs.focusDirection("left");
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-focus-down",
      section: "advanced",
      label: "Focus pane down",
      subtitle: `${formatShortcut("Ctrl+; j")} — move focus down`,
      keywords: "focus pane down split hjkl window",
      advanced: true,
      run: (ctx) => {
        shellTabs.focusDirection("down");
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-focus-up",
      section: "advanced",
      label: "Focus pane up",
      subtitle: `${formatShortcut("Ctrl+; k")} — move focus up`,
      keywords: "focus pane up split hjkl window",
      advanced: true,
      run: (ctx) => {
        shellTabs.focusDirection("up");
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-focus-right",
      section: "advanced",
      label: "Focus pane right",
      subtitle: `${formatShortcut("Ctrl+; l")} — move focus right`,
      keywords: "focus pane right next split hjkl window",
      advanced: true,
      run: (ctx) => {
        shellTabs.focusDirection("right");
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-zoom",
      section: "advanced",
      label: "Zoom pane",
      subtitle: `${formatShortcut("Ctrl+; z")} — maximize / restore active pane`,
      keywords: "zoom pane maximize tmux window",
      advanced: true,
      run: (ctx) => {
        shellTabs.zoomToggle();
        ctx.callbacks.close();
      },
    },
    {
      id: "content-zoom-in",
      section: "advanced",
      label: "Zoom in",
      subtitle: `${formatShortcut("+")} — whole UI, like the browser`,
      keywords: "zoom content font size larger scale editor ui",
      advanced: true,
      run: (ctx) => {
        stepContentZoom(1);
        ctx.callbacks.close();
      },
    },
    {
      id: "content-zoom-out",
      section: "advanced",
      label: "Zoom out",
      subtitle: `${formatShortcut("−")} — whole UI, like the browser`,
      keywords: "zoom content font size smaller scale editor ui",
      advanced: true,
      run: (ctx) => {
        stepContentZoom(-1);
        ctx.callbacks.close();
      },
    },
    {
      id: "content-zoom-reset",
      section: "advanced",
      label: "Reset zoom",
      subtitle: `${formatShortcut("0")} — back to 100%`,
      keywords: "zoom content reset default 100 ui",
      advanced: true,
      run: (ctx) => {
        resetContentZoom();
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-close",
      section: "advanced",
      label: "Close pane",
      subtitle: `${formatShortcut("Ctrl+; x")} — merge tabs into the nearest pane`,
      keywords: "close pane merge split window tabs",
      advanced: true,
      run: (ctx) => {
        shellTabs.closeActiveGroup();
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-new-chat",
      section: "advanced",
      label: "New chat in pane",
      subtitle: `${formatShortcut("Ctrl+; c")} — open chat tab in the active pane`,
      keywords: "chat pane new session tab window",
      advanced: true,
      run: (ctx) => {
        shellTabs.openDestination("chat");
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-next-tab",
      section: "advanced",
      label: "Next tab in pane",
      subtitle: `${formatShortcut("Ctrl+; n")} — cycle tabs forward`,
      keywords: "tab next pane cycle window",
      advanced: true,
      run: (ctx) => {
        shellTabs.nextTabInActiveGroup();
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-prev-tab",
      section: "advanced",
      label: "Previous tab in pane",
      subtitle: `${formatShortcut("Ctrl+; p")} — cycle tabs backward`,
      keywords: "tab previous prev pane cycle window",
      advanced: true,
      run: (ctx) => {
        shellTabs.prevTabInActiveGroup();
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-show-tabs",
      section: "advanced",
      label: "Show pane tabs",
      subtitle: `${formatShortcut("Ctrl+; w")} — briefly reveal the tab strip`,
      keywords: "tabs show flash reveal pane window",
      advanced: true,
      run: (ctx) => {
        shellTabs.flashTabs();
        ctx.callbacks.close();
      },
    },
    {
      id: "keyboard-shortcuts",
      section: "advanced",
      label: "Keyboard shortcuts",
      subtitle: `${formatShortcut("Ctrl+; ?")} — panes, vault, global binds`,
      keywords:
        "cheat sheet help shortcuts pane tmux window keyboard binds hotkeys keymap bindings",
      advanced: true,
      run: (ctx) => {
        shellTabs.requestCheatSheet();
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-toggle-rail",
      section: "advanced",
      label: "Toggle left rail",
      subtitle: `${formatShortcut("Ctrl+B")} — show or hide the master rail`,
      keywords: "sidebar rail toggle vscode cursor panel window",
      advanced: true,
      run: (ctx) => {
        layout.toggleShellSidebarExpanded();
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-summon-view-toolbar",
      section: "advanced",
      label: "Summon view toolbar",
      subtitle: `${formatShortcut("Ctrl+Shift+.")} — compact toolbar at the cursor (or shake the mouse)`,
      keywords: "toolbar summon shake mouse hud library automations chat rail popover",
      advanced: true,
      run: (ctx) => {
        summonViewToolbar();
        ctx.callbacks.close();
      },
    },
    {
      id: "desktop-flow-toolbar",
      section: "advanced",
      label: "Toggle desktop toolbar",
      subtitle: "Floating chat / note / web / views strip",
      keywords: "desktop toolbar floating popout widget flow chat note web views",
      advanced: true,
      run: async (ctx) => {
        if (!isTauri()) {
          toast.show("Desktop toolbar needs the Medousa app", { durationMs: 1600 });
          ctx.callbacks.close();
          return;
        }
        const visible = await toggleDesktopToolbar();
        toast.show(visible ? "Desktop toolbar on" : "Desktop toolbar off", {
          durationMs: 1400,
        });
        ctx.callbacks.close();
      },
    },
    {
      id: "open-operators-guide",
      section: "advanced",
      label: "Open Operator's Guide",
      subtitle: "In-app manual for navigation, chat, themes, and more",
      keywords: "help wiki docs manual guide operators handbook documentation ?",
      advanced: false,
      run: async (ctx) => {
        await openGuide();
        ctx.callbacks.close();
      },
    },
    {
      id: "pane-toggle-mouse-shake-toolbar",
      section: "advanced",
      label: isMouseShakeToolbarEnabled()
        ? "Disable shake to summon toolbar"
        : "Enable shake to summon toolbar",
      subtitle: "Mouse-shake gesture for the view toolbar HUD",
      keywords: "shake mouse toolbar gesture preference disable enable",
      advanced: true,
      run: (ctx) => {
        const enabled = toggleMouseShakeToolbarEnabled();
        toast.show(
          enabled ? "Shake to summon toolbar on" : "Shake to summon toolbar off",
          { durationMs: 1600 },
        );
        ctx.callbacks.close();
      },
    },
  ];
}

export function buildBrowserCommands(): WorkshopCommand[] {
  return [
    {
      id: "browser-focus-url",
      section: "open",
      label: "Focus address bar",
      subtitle: `${formatShortcut("L")} — select the URL / search field`,
      keywords: "browser url address bar omnibox focus swap search web",
      run: (ctx) => {
        ctx.navigate("web");
        dispatchBrowserFocusUrl();
        ctx.callbacks.close();
      },
    },
    {
      id: "browser-new-tab",
      section: "open",
      label: "New browser tab",
      subtitle: `${formatShortcut("T")} — open a blank tab`,
      keywords: "browser new tab blank web",
      run: async (ctx) => {
        ctx.navigate("web");
        await humanBrowser.openTab();
        ctx.callbacks.close();
      },
    },
    {
      id: "browser-bookmarks",
      section: "open",
      label: "Open bookmarks",
      subtitle: `${formatShortcut("⇧B")} — history, bookmarks, and library saves`,
      keywords: "browser bookmarks saved favorites history web",
      run: (ctx) => {
        ctx.navigate("web");
        dispatchBrowserOpenBookmarks();
        ctx.callbacks.close();
      },
    },
    {
      id: "browser-find-in-page",
      section: "open",
      label: "Find in page",
      subtitle: `${formatShortcut("F")} — search text on the current page`,
      keywords: "browser find search page web",
      run: (ctx) => {
        ctx.navigate("web");
        humanBrowser.openFindBar();
        ctx.callbacks.close();
      },
    },
    {
      id: "browser-open-external",
      section: "open",
      label: "Open current tab in default browser",
      subtitle: humanBrowser.activeUrl && humanBrowser.activeUrl !== "about:blank"
        ? humanBrowser.activeUrl
        : "No page loaded",
      keywords: "browser safari chrome external open tab link",
      run: async (ctx) => {
        const url = humanBrowser.activeUrl;
        if (!url || url === "about:blank") {
          ctx.error("No page to open.");
          return;
        }
        ctx.navigate("web");
        const ok = await openUrlInDefaultBrowser(url);
        if (!ok) {
          ctx.error("Could not open in default browser.");
          return;
        }
        ctx.callbacks.close();
      },
    },
    {
      id: "browser-open-clipboard",
      section: "open",
      label: "Open in browser",
      subtitle: "Navigate to a URL from the clipboard",
      keywords: "browser url clipboard link open web navigate",
      prompt: {
        placeholder: "Paste URL or search query",
        submitLabel: "Open",
      },
      run: async (ctx, args) => {
        let input = args?.trim();
        if (!input) {
          try {
            input = (await navigator.clipboard.readText()).trim();
          } catch {
            // Clipboard unavailable — user can paste in the prompt.
          }
        }
        if (!input) {
          ctx.error("Enter a URL or search query.");
          return;
        }
        ctx.navigate("web");
        await humanBrowser.navigate(input);
        ctx.callbacks.close();
        ctx.notice("Opened in browser.");
      },
    },
    {
      id: "browser-reopen-closed-tab",
      section: "open",
      label: "Reopen closed tab",
      subtitle: "Restore the last closed browser tab",
      keywords: "browser tab reopen closed undo web",
      run: async (ctx) => {
        ctx.navigate("web");
        if (humanBrowser.closedTabs.length === 0) {
          ctx.error("No closed tab to reopen.");
          return;
        }
        await humanBrowser.reopenClosedTab();
        ctx.callbacks.close();
      },
    },
    {
      id: "browser-copy-url",
      section: "open",
      label: "Copy browser link",
      subtitle: "Copy the current tab URL",
      keywords: "browser copy url link clipboard web",
      run: async (ctx) => {
        const url = humanBrowser.activeUrl;
        if (!url || url === "about:blank") {
          ctx.error("No page to copy.");
          return;
        }
        const ok = await copyBrowserUrl(url);
        if (!ok) {
          ctx.error("Could not copy link.");
          return;
        }
        ctx.notice("Link copied.");
        ctx.callbacks.close();
      },
    },
  ];
}

export function buildLibraryCommands(): WorkshopCommand[] {
  return [
    {
      id: "open-loose-markdown",
      section: "open",
      label: "Open markdown file…",
      subtitle: "Edit a single .md without adding a vault folder",
      keywords: "open file markdown md loose note document",
      run: async (ctx) => {
        const { canUseLocalVaultFilesystem } = await import("$lib/utils/vaultFilesystem");
        if (!canUseLocalVaultFilesystem()) {
          ctx.error(`Open markdown file needs the desktop app ${onThisHostPhrase()}.`);
          return;
        }
        ctx.navigate("library");
        const opened = await ctx.vault.openLooseMarkdownFile();
        if (opened) ctx.callbacks.close();
      },
    },
  ];
}

export function buildAskCommands(): WorkshopCommand[] {
  return [
    {
      id: "ask-focus",
      section: "ask",
      label: "Write a new message",
      subtitle: "Jump to chat composer",
      keywords: "focus chat compose message",
      run: (ctx) => {
        ctx.navigate("chat");
        ctx.callbacks.focusChat();
        ctx.callbacks.close();
      },
    },
    {
      id: "ask-new-session",
      section: "ask",
      label: "Start fresh conversation",
      subtitle: "New chat session",
      keywords: "new session chat conversation",
      run: async (ctx) => {
        await ctx.chat.newSession();
        ctx.navigate("chat");
        ctx.callbacks.focusChat();
        ctx.callbacks.close();
        ctx.notice("Started a new conversation.");
      },
    },
    {
      id: "ask-background",
      section: "ask",
      label: "Background task",
      subtitle: "Medousa works while you keep chatting",
      keywords: "ask background job daemon",
      prompt: {
        placeholder: "What should Medousa work on in the background?",
        submitLabel: "Start task",
      },
      run: async (ctx, prompt) => {
        const text = prompt?.trim();
        if (!text) {
          ctx.error("Describe what Medousa should work on.");
          return;
        }
        const opts = buildInteractiveTurnOptions();
        const accepted = await createTurnTicket({
          sessionId: ctx.chat.sessionId,
          prompt: text,
          mode: "background",
          provider: opts.provider,
          model: opts.model,
          responseDepthMode: opts.responseDepthMode,
          reasoningEffort: opts.reasoningEffort,
          stageRouting: opts.stageRouting,
          channelSurface: opts.channelSurface,
          identityUserId: opts.identityUserId,
        });
        ctx.chat.beginTurn(text, accepted, [], opts.identityUserId);
        await ctx.chat.startTurnStream(
          accepted.turn_id,
          accepted.session_id,
          accepted.stream_url,
        );
        ctx.navigate("chat");
        ctx.callbacks.close();
        ctx.notice("Background task started.");
      },
    },
  ];
}

export function buildTuneCommands(): WorkshopCommand[] {
  const depthOptions: { mode: DepthMode; label: string }[] = [
    { mode: "concise", label: "Concise answers" },
    { mode: "standard", label: "Standard depth" },
    { mode: "deep", label: "Deep answers" },
  ];

  return [
    {
      id: "tune-models-settings",
      section: "tune",
      label: "Change model",
      subtitle: "Medousa Agent — models & stages",
      keywords: "model provider settings reasoning effort stages agent",
      run: (ctx) => {
        ctx.openSettingsSection("agent");
        ctx.callbacks.close();
      },
    },
    {
      id: "tune-engine-settings",
      section: "tune",
      label: "Runtime controls",
      subtitle: "Reach, shell, budgets & diagnostics",
      keywords: "engine budgets quality diagnostics verifier reach shell runtime",
      run: (ctx) => {
        ctx.openSettingsSection("runtime");
        ctx.callbacks.close();
      },
    },
    {
      id: "tune-voice-settings",
      section: "tune",
      label: "Voice and stance",
      subtitle: "Medousa Agent — stance & depth",
      keywords: "voice stance depth charter agent memory",
      run: (ctx) => {
        ctx.openSettingsSection("agent");
        ctx.callbacks.close();
      },
    },
    ...depthOptions.map(
      (option): WorkshopCommand => ({
        id: `tune-depth-${option.mode}`,
        section: "tune",
        label: option.label,
        subtitle: "Answer depth for the next turns",
        keywords: `depth ${option.mode} stance`,
        run: async (ctx) => {
          await ctx.runtime.setDepthMode(option.mode);
          ctx.notice(`Answer depth set to ${option.mode}.`);
          ctx.callbacks.close();
        },
      }),
    ),
  ];
}

export function buildAdvancedCommands(): WorkshopCommand[] {
  return [
    {
      id: "advanced-help",
      section: "advanced",
      label: "Show chat slash commands",
      subtitle: "Operator shortcuts in chat",
      hint: "/help",
      keywords: "help slash commands",
      advanced: true,
      run: (ctx) => {
        ctx.notice(
          "/ask … · /budget list · /budget approve · /budget deny · /usage · /help",
        );
      },
    },
    {
      id: "advanced-budget-list",
      section: "advanced",
      label: "List budget approvals",
      subtitle: "Pending tool-round extensions",
      hint: "/budget list",
      keywords: "budget list pending approve",
      advanced: true,
      run: async (ctx) => {
        const rows = await listTurnBudgetRequests(true);
        if (rows.length === 0) {
          ctx.notice("No pending budget approvals.");
          return;
        }
        ctx.notice(
          rows
            .map(
              (row) =>
                `${row.request_id.slice(0, 8)}… +${row.requested_rounds} rounds`,
            )
            .join(" · "),
        );
      },
    },
    {
      id: "advanced-health",
      section: "advanced",
      label: "Check daemon health",
      subtitle: "Connection and backend status",
      keywords: "health daemon doctor status",
      advanced: true,
      run: async (ctx) => {
        try {
          const health = await checkDaemonHealth();
          ctx.openRuntimeTab("jobs");
          ctx.notice(
            health.ok
              ? `Daemon healthy — ${health.message ?? "ok"}`
              : `Daemon issue — ${health.message ?? "offline"}`,
          );
          ctx.callbacks.close();
        } catch (err) {
          ctx.error(err instanceof Error ? err.message : String(err));
        }
      },
    },
    {
      id: "advanced-app-update",
      section: "advanced",
      label: "Check for updates",
      subtitle: "Desktop app vs release channel",
      keywords: "update upgrade download version release app",
      advanced: true,
      run: async (ctx) => {
        if (!isTauri()) {
          ctx.notice("Updates are available in the Medousa desktop app.");
          ctx.callbacks.close();
          return;
        }
        const status = await appUpdate.check();
        ctx.openSettingsSection("basement");
        if (!status) {
          ctx.error("Could not reach the release channel.");
          ctx.callbacks.close();
          return;
        }
        if (status.error && !status.latestVersion) {
          ctx.error(status.error);
          ctx.callbacks.close();
          return;
        }
        if (status.updateAvailable && status.latestVersion) {
          ctx.notice(`Update available · ${status.latestVersion}`);
          try {
            await openAppUpdateDownload();
          } catch {
            /* Settings card still shows Download */
          }
        } else {
          ctx.notice(`You’re up to date · ${status.currentVersion}`);
        }
        ctx.callbacks.close();
      },
    },
    {
      id: "advanced-skills",
      section: "advanced",
      label: "List skills",
      subtitle: "Imported skill manuscripts",
      keywords: "skills manuscripts catalog",
      advanced: true,
      run: async (ctx) => {
        const catalog = await listManuscripts({ skillsOnly: true, limit: 24 });
        const items = catalog.manuscripts ?? [];
        if (items.length === 0) {
          ctx.notice("No skills imported yet.");
          return;
        }
        ctx.notice(
          items
            .slice(0, 12)
            .map((item) => item.name || item.id)
            .filter(Boolean)
            .join(" · "),
        );
        ctx.navigate("workshop");
        ctx.callbacks.close();
      },
    },
    {
      id: "advanced-stage-routes",
      section: "advanced",
      label: "Edit stage routes",
      subtitle: "Stage models in Settings → Medousa Agent",
      keywords: "stage routes routing matrix specialists models agent",
      advanced: true,
      run: (ctx) => {
        ctx.openSettingsSection("agent");
        ctx.callbacks.close();
      },
    },
    {
      id: "advanced-export-session",
      section: "advanced",
      label: "Export this conversation",
      subtitle: "Download session history as Markdown",
      keywords: "export session history markdown md transcript",
      advanced: true,
      run: async (ctx) => {
        const history = await getSessionHistory(ctx.chat.sessionId);
        const label =
          ctx.chat.sessions.find((s) => s.session_id === ctx.chat.sessionId)
            ?.display_name ||
          ctx.chat.sessions.find((s) => s.session_id === ctx.chat.sessionId)
            ?.preview ||
          undefined;
        const markdown = sessionTranscriptMarkdown(history, { title: label });
        downloadTextFile(
          `${sessionExportBasename(ctx.chat.sessionId)}.md`,
          markdown,
        );
        ctx.notice("Conversation exported as Markdown.");
        ctx.callbacks.close();
      },
    },
    {
      id: "advanced-export-session-pdf",
      section: "advanced",
      label: "Export conversation as PDF…",
      subtitle: "Preview and save a PDF of this chat",
      keywords: "export session history pdf transcript",
      advanced: true,
      run: async (ctx) => {
        const history = await getSessionHistory(ctx.chat.sessionId);
        const label =
          ctx.chat.sessions.find((s) => s.session_id === ctx.chat.sessionId)
            ?.display_name ||
          ctx.chat.sessions.find((s) => s.session_id === ctx.chat.sessionId)
            ?.preview ||
          `Conversation ${ctx.chat.sessionId.slice(0, 8)}`;
        const markdown = sessionTranscriptMarkdown(history, { title: label });
        sessionExportPreview.show(label, markdown);
        ctx.callbacks.close();
      },
    },
    {
      id: "advanced-export-session-json",
      section: "advanced",
      label: "Export conversation as JSON",
      subtitle: "Raw session history for debugging",
      keywords: "export session history json",
      advanced: true,
      run: async (ctx) => {
        const history = await getSessionHistory(ctx.chat.sessionId);
        downloadTextFile(
          `${sessionExportBasename(ctx.chat.sessionId)}.json`,
          JSON.stringify(history, null, 2),
          "application/json",
        );
        ctx.notice("Session exported as JSON.");
        ctx.callbacks.close();
      },
    },
    {
      id: "advanced-usage",
      section: "advanced",
      label: "See context usage",
      subtitle: "Token breakdown for the last turn",
      hint: "/usage",
      keywords: "context usage tokens budget",
      advanced: true,
      run: (ctx) => {
        if (!ctx.chat.contextUsage) {
          ctx.notice("No context usage snapshot yet — send a message first.");
          return;
        }
        ctx.navigate("chat");
        ctx.chat.contextUsagePanelOpen = true;
        ctx.callbacks.close();
      },
    },
  ];
}

export async function runBudgetApprove(
  ctx: WorkshopCommandContext,
  requestId?: string,
) {
  const id =
    requestId?.trim() ||
    ctx.chat.pendingBudgetApprovals[0]?.requestId ||
    ctx.chat.budgetAlert?.requestId;
  if (!id) {
    ctx.error("No pending budget approval.");
    return;
  }
  const pending =
    ctx.chat.pendingBudgetApprovals.find((item) => item.requestId === id) ??
    ctx.chat.budgetAlert;
  await approveTurnBudgetRequest(
    id,
    pending?.requestedRounds ?? undefined,
    homeChannelSurface(),
  );
  ctx.chat.noteBudgetResolved(id);
  ctx.chat.clearBudgetAlert();
  ctx.notice("Approved — turn resuming.");
  ctx.callbacks.close();
}

export async function runBudgetDeny(ctx: WorkshopCommandContext, requestId?: string) {
  const id =
    requestId?.trim() ||
    ctx.chat.pendingBudgetApprovals[0]?.requestId ||
    ctx.chat.budgetAlert?.requestId;
  if (!id) {
    ctx.error("No pending budget approval.");
    return;
  }
  await denyTurnBudgetRequest(id, homeChannelSurface());
  ctx.chat.noteBudgetResolved(id);
  ctx.chat.clearBudgetAlert();
  ctx.notice("Budget request denied.");
  ctx.callbacks.close();
}

export async function runReconnect(ctx: WorkshopCommandContext) {
  connection.setRecovering(true);
  try {
    const health = await reconnectWorkshop((next) => connection.setHealth(next));
    ctx.notice(health.ok ? "Workshop reconnected." : health.message ?? "Still offline.");
    if (health.ok) ctx.callbacks.close();
  } finally {
    connection.setRecovering(false);
  }
}
