import {
  runBudgetApprove,
  runBudgetDeny,
} from "./registry";
import { recordCommandUsage } from "./usage";
import type { WorkshopCommandContext } from "./types";

export type WorkshopCommandId =
  | "help"
  | "budget.list"
  | "budget.approve"
  | "budget.deny"
  | "usage.toggle";

export interface WorkshopCommandInvoke {
  id: WorkshopCommandId;
  requestId?: string;
}

export async function runWorkshopCommand(
  ctx: WorkshopCommandContext,
  command: WorkshopCommandInvoke,
): Promise<void> {
  switch (command.id) {
    case "help":
      ctx.notice(
        "/ask … · /budget list · /budget approve · /budget deny · /usage · /help",
      );
      return;
    case "budget.list": {
      const { listTurnBudgetRequests } = await import("$lib/daemon");
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
      return;
    }
    case "budget.approve":
      await runBudgetApprove(ctx, command.requestId);
      return;
    case "budget.deny":
      await runBudgetDeny(ctx, command.requestId);
      return;
    case "usage.toggle":
      if (!ctx.chat.contextUsage) {
        ctx.notice("No context usage snapshot yet — send a message first.");
        return;
      }
      ctx.chat.contextUsagePanelOpen = !ctx.chat.contextUsagePanelOpen;
      return;
  }
}

export async function executeWorkshopCommand(
  ctx: WorkshopCommandContext,
  command: import("./types").WorkshopCommand,
  args?: string,
): Promise<void> {
  recordCommandUsage(command.id);
  await command.run(ctx, args);
}
