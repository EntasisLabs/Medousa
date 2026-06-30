import {
  approveTurnBudgetRequest,
  checkDaemonHealth,
  denyTurnBudgetRequest,
  enqueueDaemonAsk,
  getSessionHistory,
  listManuscripts,
  listTurnBudgetRequests,
  sendStageRouteCommand,
} from "$lib/daemon";
import { homeChannelSurface } from "$lib/platform";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { copyBrowserUrl, openUrlInDefaultBrowser } from "$lib/utils/browserActions";
import { reconnectWorkshop } from "$lib/workshopConnection";
import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
import { createTurnTicket } from "$lib/daemon";
import { connection } from "$lib/stores/connection.svelte";
import type { Surface } from "$lib/types/ui";
import type { DepthMode } from "$lib/types/runtime";
import type { WorkshopCommand, WorkshopCommandContext } from "./types";

const GO_DESTINATIONS: { surface: Surface; label: string; subtitle: string; keywords: string }[] = [
  { surface: "chat", label: "Chat", subtitle: "Talk with Medousa", keywords: "message compose conversation" },
  { surface: "library", label: "Library", subtitle: "Notes and documents", keywords: "vault notes documents" },
  { surface: "work", label: "Work", subtitle: "Tasks and kanban board", keywords: "kanban cards jobs" },
  { surface: "web", label: "Browser", subtitle: "Built-in web workshop", keywords: "browser web surf" },
  { surface: "automations", label: "Automations", subtitle: "Scripts and schedules", keywords: "cron scripts grapheme" },
  { surface: "workshop", label: "Capabilities", subtitle: "Skills and specialist workspaces", keywords: "skills manuscripts workshop capabilities specialist" },
  { surface: "context", label: "Context map", subtitle: "Memory and threads", keywords: "memory locus context" },
  { surface: "profiles", label: "Profiles", subtitle: "People and identity", keywords: "profiles identity people" },
  { surface: "messaging", label: "Messaging", subtitle: "WhatsApp, Telegram, and more", keywords: "channels telegram whatsapp" },
  { surface: "runtime", label: "Engine status", subtitle: "Jobs, delivery, health", keywords: "runtime engine daemon health" },
  { surface: "settings", label: "Settings", subtitle: "Preferences and connection", keywords: "settings preferences config" },
];

export function buildGoCommands(): WorkshopCommand[] {
  return GO_DESTINATIONS.map((dest) => ({
    id: `go-${dest.surface}`,
    section: "go",
    label: dest.label,
    subtitle: dest.subtitle,
    keywords: dest.keywords,
    run: (ctx) => {
      ctx.navigate(dest.surface);
      ctx.callbacks.close();
    },
  }));
}

export function buildBrowserCommands(): WorkshopCommand[] {
  return [
    {
      id: "browser-find-in-page",
      section: "open",
      label: "Find in page",
      subtitle: "Search text on the current page",
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
        ctx.chat.beginTurn(text, accepted, []);
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
    {
      id: "ask-morning-brief",
      section: "ask",
      label: "Morning brief",
      subtitle: "Run the morning-brief manuscript",
      keywords: "brief morning digest summary",
      run: async (ctx) => {
        await enqueueDaemonAsk({
          prompt: "Run the morning brief.",
          manuscriptId: "morning-brief",
        });
        ctx.navigate("work");
        ctx.callbacks.close();
        ctx.notice("Morning brief queued.");
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
      id: "tune-engine-controls",
      section: "tune",
      label: "Open engine controls",
      subtitle: "Model, depth, and routing",
      keywords: "runtime controls model depth routing",
      run: (ctx) => {
        ctx.openRuntimeTab("controls");
        ctx.callbacks.close();
      },
    },
    {
      id: "tune-models-settings",
      section: "tune",
      label: "Change model",
      subtitle: "Open Models in Settings",
      keywords: "model provider settings",
      run: (ctx) => {
        ctx.openSettingsSection("models");
        ctx.callbacks.close();
      },
    },
    {
      id: "tune-voice-settings",
      section: "tune",
      label: "Voice and stance",
      subtitle: "Open Voice in Settings",
      keywords: "voice stance depth charter",
      run: (ctx) => {
        ctx.openSettingsSection("voice");
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
      label: "Show stage routes",
      subtitle: "Read-only routing matrix summary",
      keywords: "stage routes routing matrix",
      advanced: true,
      run: async (ctx) => {
        const response = await sendStageRouteCommand({
          stage_routing: ctx.runtime.stageRouting,
          provider: ctx.runtime.provider,
          model: ctx.runtime.model,
          command: { command: "routes", role: null },
        });
        const routes = [
          response.stage_routing.orchestrator,
          response.stage_routing.final_response,
        ]
          .map((route) => `${route.role}: ${route.provider}:${route.model}`)
          .join(" · ");
        ctx.openRuntimeTab("routing");
        ctx.notice(routes || "Stage routes loaded.");
        ctx.callbacks.close();
      },
    },
    {
      id: "advanced-export-session",
      section: "advanced",
      label: "Export this conversation",
      subtitle: "Download session history as JSON",
      keywords: "export session history json",
      advanced: true,
      run: async (ctx) => {
        const history = await getSessionHistory(ctx.chat.sessionId);
        const blob = new Blob([JSON.stringify(history, null, 2)], {
          type: "application/json",
        });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = `medousa-session-${ctx.chat.sessionId.slice(0, 8)}.json`;
        anchor.click();
        URL.revokeObjectURL(url);
        ctx.notice("Session exported.");
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
