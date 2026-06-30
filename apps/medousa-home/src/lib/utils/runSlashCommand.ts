import {
  approveTurnBudgetRequest,
  denyTurnBudgetRequest,
  listTurnBudgetRequests,
} from "$lib/daemon";
import { chat } from "$lib/stores/chat.svelte";
import { homeChannelSurface } from "$lib/platform";
import {
  parseSlashCommand,
  SLASH_COMMAND_HINTS,
  type SlashCommandResult,
} from "$lib/utils/slashCommands";

export function parseChatSlashInput(value: string): SlashCommandResult | null {
  return parseSlashCommand(value);
}

export async function runSlashCommand(command: SlashCommandResult): Promise<void> {
  switch (command.kind) {
    case "help":
      chat.historyNotice = SLASH_COMMAND_HINTS.join(" · ");
      return;
    case "budget_list": {
      const rows = await listTurnBudgetRequests(true);
      if (rows.length === 0) {
        chat.historyNotice = "No pending budget approvals.";
        return;
      }
      chat.historyNotice = rows
        .map(
          (row) =>
            `${row.request_id.slice(0, 8)}… +${row.requested_rounds} rounds`,
        )
        .join(" · ");
      return;
    }
    case "budget_approve": {
      const requestId =
        command.requestId?.trim() ||
        chat.pendingBudgetApprovals[0]?.requestId ||
        chat.budgetAlert?.requestId;
      if (!requestId) {
        chat.setError("No pending budget approval — try /budget list");
        return;
      }
      const pending =
        chat.pendingBudgetApprovals.find((item) => item.requestId === requestId) ??
        chat.budgetAlert;
      await approveTurnBudgetRequest(
        requestId,
        pending?.requestedRounds ?? undefined,
        homeChannelSurface(),
      );
      chat.noteBudgetResolved(requestId);
      chat.clearBudgetAlert();
      chat.historyNotice = "Budget approved — turn resuming.";
      return;
    }
    case "budget_deny": {
      const requestId =
        command.requestId?.trim() ||
        chat.pendingBudgetApprovals[0]?.requestId ||
        chat.budgetAlert?.requestId;
      if (!requestId) {
        chat.setError("No pending budget approval — try /budget list");
        return;
      }
      await denyTurnBudgetRequest(requestId, homeChannelSurface());
      chat.noteBudgetResolved(requestId);
      chat.clearBudgetAlert();
      chat.historyNotice = "Budget request denied.";
      return;
    }
    case "usage": {
      if (!chat.contextUsage) {
        chat.historyNotice =
          "No context usage snapshot yet — send a message first.";
        return;
      }
      chat.contextUsagePanelOpen = !chat.contextUsagePanelOpen;
      return;
    }
    default:
      return;
  }
}
