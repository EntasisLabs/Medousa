import type { SlashCommandResult } from "$lib/utils/slashCommands";
import type { WorkshopCommandInvoke } from "./runWorkshopCommand";

export function slashToWorkshopCommand(
  slash: SlashCommandResult,
): WorkshopCommandInvoke | null {
  switch (slash.kind) {
    case "help":
      return { id: "help" };
    case "budget_list":
      return { id: "budget.list" };
    case "budget_approve":
      return { id: "budget.approve", requestId: slash.requestId };
    case "budget_deny":
      return { id: "budget.deny", requestId: slash.requestId };
    case "usage":
      return { id: "usage.toggle" };
    case "ask":
      return null;
    default:
      return null;
  }
}
