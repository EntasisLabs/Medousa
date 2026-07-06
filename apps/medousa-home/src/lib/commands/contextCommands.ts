import type { WorkshopCommand, WorkshopCommandContext } from "./types";
import {
  runBudgetApprove,
  runBudgetDeny,
  runReconnect,
} from "./registry";
import { buildRecentSessionCommands } from "./searchProviders";

export function buildSuggestedCommands(ctx: WorkshopCommandContext): WorkshopCommand[] {
  const commands: WorkshopCommand[] = [];

  if (ctx.connection.offline) {
    commands.push({
      id: "suggested-reconnect",
      section: "suggested",
      label: "Reconnect workshop",
      subtitle: "Medousa is offline",
      keywords: "reconnect offline connection",
      risk: "attention",
      run: () => runReconnect(ctx),
    });
  }

  for (const item of ctx.chat.pendingBudgetApprovals) {
    commands.push(
      {
        id: `suggested-approve-${item.requestId}`,
        section: "suggested",
        label: `Give more thinking time (+${item.requestedRounds ?? "?"})`,
        subtitle: "Medousa paused mid-task",
        hint: item.requestId.slice(0, 8),
        keywords: `budget approve ${item.requestId}`,
        run: () => runBudgetApprove(ctx, item.requestId),
      },
      {
        id: `suggested-deny-${item.requestId}`,
        section: "suggested",
        label: "Stop this task",
        subtitle: "Deny extra tool rounds",
        hint: item.requestId.slice(0, 8),
        keywords: `budget deny ${item.requestId}`,
        risk: "attention",
        run: () => runBudgetDeny(ctx, item.requestId),
      },
      {
        id: `suggested-work-${item.requestId}`,
        section: "suggested",
        label: "Review approval in Work",
        subtitle: "Open the related work card",
        keywords: `work card ${item.requestId}`,
        run: async (runCtx) => {
          runCtx.workspace.workView = "kanban";
          runCtx.navigate("work");
          await runCtx.workspace.selectCard(item.workCardId);
          runCtx.callbacks.close();
        },
      },
    );
  }

  if (ctx.chat.contextUsage) {
    commands.push({
      id: "suggested-context-usage",
      section: "suggested",
      label: "See context usage",
      subtitle: "How full is this turn",
      keywords: "context usage tokens",
      run: (runCtx) => {
        runCtx.navigate("chat");
        runCtx.chat.contextUsagePanelOpen = true;
        runCtx.callbacks.close();
      },
    });
  }

  if (ctx.chat.liveStreamActive) {
    commands.push({
      id: "suggested-live-turn",
      section: "suggested",
      label: "Go to live turn",
      subtitle: "Medousa is responding now",
      keywords: "live stream turn chat",
      run: (runCtx) => {
        runCtx.navigate("chat");
        runCtx.callbacks.focusChat();
        window.dispatchEvent(
          new CustomEvent("medousa-chat-scroll-to-bottom", { detail: { force: true } }),
        );
        runCtx.callbacks.close();
      },
    });
  }

  const blocked = ctx.workspace.cards.filter((card) => card.column === "blocked");
  if (blocked.length > 0) {
    const first = blocked[0];
    commands.push({
      id: "suggested-blocked-work",
      section: "suggested",
      label: "See blocked work",
      subtitle: `${blocked.length} item${blocked.length === 1 ? "" : "s"} need attention`,
      keywords: "blocked work kanban",
      run: async (runCtx) => {
        runCtx.workspace.workView = "kanban";
        runCtx.navigate("work");
        await runCtx.workspace.selectCard(first.id);
        runCtx.callbacks.close();
      },
    });
  }

  if (commands.length === 0) {
    commands.push(...buildRecentSessionCommands(ctx));
  }

  return commands;
}

export function buildBudgetListCommand(ctx: WorkshopCommandContext): WorkshopCommand | null {
  if (ctx.chat.pendingBudgetApprovals.length > 0) return null;
  return {
    id: "budget-list",
    section: "suggested",
    label: "Check pending approvals",
    subtitle: "Tool-round budget queue",
    hint: "/budget list",
    keywords: "budget list pending approve",
    run: async (runCtx) => {
      const { listTurnBudgetRequests } = await import("$lib/daemon");
      const rows = await listTurnBudgetRequests(true);
      if (rows.length === 0) {
        runCtx.notice("No pending budget approvals.");
        return;
      }
      runCtx.notice(
        rows
          .map(
            (row) =>
              `${row.request_id.slice(0, 8)}… +${row.requested_rounds} rounds`,
          )
          .join(" · "),
      );
    },
  };
}
