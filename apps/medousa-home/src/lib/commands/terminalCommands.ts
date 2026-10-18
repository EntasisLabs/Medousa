import { isTauriDesktop } from "$lib/platform";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import { openWorkshopTerminal, openSshTerminal } from "$lib/utils/terminalActions";
import type { SshTarget } from "$lib/utils/sshApi";
import type { WorkshopCommand } from "./types";

export function buildTerminalCommands(): WorkshopCommand[] {
  if (!isTauriDesktop()) return [];
  const scope = activeWorkshopId();
  return [
    {
      id: "terminal-open", section: "do", label: "Open terminal",
      subtitle: "New shell on the connected workshop", keywords: "terminal shell console new open",
      aliases: ["Open terminal", "Terminal", "Shell"],
      run: async (ctx) => { await openWorkshopTerminal(scope); ctx.callbacks.close(); },
    },
    {
      id: "terminal-open-ssh", section: "do", label: "Open SSH terminal…",
      subtitle: "Choose a saved server", keywords: "ssh terminal shell server remote connect",
      aliases: ["Open SSH terminal", "SSH terminal"],
      run: () => commandSpotlight.openSshTargets(),
    },
  ];
}

export function buildSshTerminalCommands(targets: SshTarget[], scope: string): WorkshopCommand[] {
  if (!isTauriDesktop()) return [];
  return targets.map((target) => ({
    id: `terminal-ssh:${target.target_id}`, section: "open", label: `SSH · ${target.name}`,
    subtitle: `${target.username}@${target.host}:${target.port}`,
    aliases: [target.name, target.host, `${target.username}@${target.host}:${target.port}`],
    keywords: `ssh terminal shell server remote connect ${target.name} ${target.username} ${target.host} ${target.port}`,
    run: async (ctx) => { await openSshTerminal(target, scope); ctx.callbacks.close(); },
  }));
}
