import { buildWorkshopCommandContext } from "$lib/commands/context";
import { runWorkshopCommand } from "$lib/commands/runWorkshopCommand";
import { slashToWorkshopCommand } from "$lib/commands/slashBridge";
import {
  parseSlashCommand,
  SLASH_COMMAND_HINTS,
  type SlashCommandResult,
} from "$lib/utils/slashCommands";

export function parseChatSlashInput(value: string): SlashCommandResult | null {
  return parseSlashCommand(value);
}

export { SLASH_COMMAND_HINTS };

export async function runSlashCommand(command: SlashCommandResult): Promise<void> {
  const mapped = slashToWorkshopCommand(command);
  if (!mapped) return;

  const ctx = buildWorkshopCommandContext({
    close: () => {},
    focusChat: () => {
      window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
    },
  });
  await runWorkshopCommand(ctx, mapped);
}
