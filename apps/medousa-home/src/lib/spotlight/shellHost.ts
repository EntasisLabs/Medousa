import { activeWorkshopId } from "$lib/utils/workshopLocality";
import { buildWorkshopCommandContext } from "$lib/commands/context";
import { collectWorkshopCommands, collectWorkshopLensCommands, parseSpotlightQuery } from "$lib/commands/collectCommands";
import { vaultSearchCatalog } from "$lib/vault/vaultSearchCatalog.svelte";
import { buildPinnedJumpCommands } from "$lib/commands/pinCommands";
import { executeWorkshopCommand } from "$lib/commands/runWorkshopCommand";
import { sshTargets } from "$lib/utils/sshApi";
import type { WorkshopCommand } from "$lib/commands/types";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workshop } from "$lib/stores/workshop.svelte";
import { bots } from "$lib/stores/bots.svelte";
import { connectedAgents } from "$lib/stores/connectedAgents.svelte";
import { spotlightPins } from "$lib/stores/spotlightPins.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { focusChatComposer } from "$lib/runtime/shellUseCases";
import { openDesktopWindow } from "$lib/desktop/windows";
import { showMainWindow } from "$lib/window";
import { openNewAsk } from "$lib/utils/workChromeEvents";
import { openActivityTarget } from "./activityNavigation";
import { collectDesktopGroups, spotlightTabKind, spotlightTabPreview, spotlightWorkspaceLayout } from "./desktopGroups";
import type { SpotlightHostRequest, SpotlightQuery, SpotlightSnapshot } from "./host";

type Ticket = { command: WorkshopCommand; scope: string; context: string; expires: number };
export type SpotlightShellHost = ReturnType<typeof createSpotlightShellHost>;
const contextKey = () => JSON.stringify([shellTabs.activeDesktopId, shellTabs.activeGroupId, shellTabs.activeTab?.id, chat.sessionId]);

/** Main-shell composition service. Both surfaces use the same command builders.
 * Only opaque, bounded-lived command tickets cross the native relay. */
export function createSpotlightShellHost() {
  const tickets = new Map<string, Ticket>();
  let sequence = 0;
  let disposed = false;
  let sourceRefresh: { scope: string; at: number; work: Promise<unknown> } | null = null;
  let sshCache: { scope: string; at: number; targets: Awaited<ReturnType<typeof sshTargets>> } | null = null;
  const ctx = () => ({
    ...buildWorkshopCommandContext({ close: () => {}, focusChat: focusChatComposer }),
    error(message: string): never { throw new Error(message); },
  });
  function requireScope(scope: string) {
    if (disposed) throw new Error("The Spotlight command owner closed");
    if (activeWorkshopId() !== scope || chat.workshopScopeId !== scope) throw new Error("The main window changed workshops. Reopen Spotlight to use its current commands; accepted work keeps running.");
  }
  async function focusShell(scope: string) {
    await showMainWindow(); requireScope(scope);
    // This is the main renderer's palette, not the desktop widget's state.
    commandSpotlight.closeSpotlight();
  }
  const describe = (command: WorkshopCommand, scope: string) => {
    const token = `${Date.now()}-${++sequence}`;
    for (const [id, ticket] of tickets) if (ticket.expires < Date.now()) tickets.delete(id);
    while (tickets.size >= 384) tickets.delete(tickets.keys().next().value!);
    tickets.set(token, { command, scope, context: contextKey(), expires: Date.now() + 120_000 });
    const { run: _run, ...metadata } = command;
    return { ...metadata, token };
  };
  async function collect(scope: string, query: SpotlightQuery): Promise<SpotlightSnapshot> {
    requireScope(scope);
    if (query.query.length > 4096) throw new Error("Shorten this search before continuing.");
    const parsed = parseSpotlightQuery(query.query);
    if (query.mode !== "ssh" && (query.mode === "notes" || ["default", "advanced"].includes(parsed.mode))) await vaultSearchCatalog.search(parsed.rawQuery);
    requireScope(scope);
    if (!sourceRefresh || sourceRefresh.scope !== scope || Date.now() - sourceRefresh.at > 15_000) {
      sourceRefresh = { scope, at: Date.now(), work: Promise.allSettled([
        vault.notes.length ? Promise.resolve() : vault.refreshNotes(),
        workshop.scripts.length ? Promise.resolve() : workshop.refreshModulesAndScripts(),
        chat.refreshSessions(), bots.refresh(), connectedAgents.refresh(scope),
      ]) };
    }
    await sourceRefresh.work;
    requireScope(scope);
    if (!sshCache || sshCache.scope !== scope || Date.now() - sshCache.at > 30_000) {
      try { sshCache = { scope, at: Date.now(), targets: await sshTargets() }; } catch { sshCache = null; }
      requireScope(scope);
    }
    const layout = spotlightWorkspaceLayout(query.scopeId);
    const groups = query.scopeId !== "home" && layout
      ? collectDesktopGroups(query.scopeId, layout, query.query, spotlightTabPreview, spotlightTabKind)
      : query.mode === "ssh" ? []
      : query.mode !== "notes" && !query.query.trim()
        ? collectWorkshopLensCommands(ctx(), query.lens).map(group => ({ id: `${query.lens}-${group.label}`, ...group }))
        : collectWorkshopCommands(ctx(), { query: query.query, notesMode: query.mode === "notes", sshTargets: sshCache?.targets, workshopScope: scope }).map(group => ({ id: `${group.section}-${group.label}`, ...group }));
    return {
      notice: vaultSearchCatalog.error,
      groups: groups.map(group => ({ id: group.id, label: group.label, commands: group.commands.map(command => describe(command, scope)) })),
      scopes: [{ id: "home", label: "Home" }, ...shellTabs.desktops.map(desktop => ({ id: desktop.id, label: desktop.name }))],
      layout,
    };
  }
  async function run(scope: string, command: WorkshopCommand, expectedContext?: string) {
    requireScope(scope);
    if (command.id === "terminal-open" || command.id.startsWith("terminal-ssh:")) {
      await openDesktopWindow({ kind: "terminal", workshopId: scope,
        ...(command.id.startsWith("terminal-ssh:") ? { sshTargetId: command.id.slice("terminal-ssh:".length) } : {}) }); return;
    }
    if (["do-create-chat", "ask-new-session"].includes(command.id)) {
      const { creativeBridgeApi } = await import("$lib/create/creativeBridge");
      const session = await creativeBridgeApi(scope).newSession("New conversation");
      requireScope(scope);
      await openDesktopWindow({ kind: "chat", workshopId: scope, resourceId: session.session_id, title: "New conversation" }); return;
    }
    if (command.id.startsWith("open-session:")) {
      await openDesktopWindow({ kind: "chat", workshopId: scope, resourceId: command.id.slice("open-session:".length), title: command.label.replace(/^Open chat:\s*/, "") }); return;
    }
    if (["go-chat", "go-code", "go-create"].includes(command.id)) {
      await openDesktopWindow({ kind: command.id.slice(3) as "chat" | "code" | "create", workshopId: scope }); return;
    }
    // Existing editor events, dialogs and tabs must execute in the real shell.
    await focusShell(scope);
    if (expectedContext && expectedContext !== contextKey()) throw new Error("The active document changed. Search for this command again.");
    await executeWorkshopCommand(ctx(), command);
  }
  return {
    async handle(scope: string, request: SpotlightHostRequest): Promise<unknown> {
      requireScope(scope);
      switch (request.kind) {
        case "collect": return collect(scope, request.query);
        case "execute": {
          const ticket = tickets.get(request.token);
          if (!ticket || ticket.scope !== scope || ticket.expires < Date.now()) throw new Error("This command expired. Search for it again.");
          tickets.delete(request.token); // A timed-out or double-clicked action cannot execute twice.
          const contextual = /^(workbench\.|editor\.|git\.|medousa\.code\.|pane-|suggested-|do-toggle|do-pin-current|advanced-export|workspace-(rename|remove))/.test(ticket.command.id);
          if (contextual && ticket.context !== contextKey()) throw new Error("The active document changed. Search for this command again.");
          await run(scope, { ...ticket.command, run: context => ticket.command.run(context, request.args) }, contextual ? ticket.context : undefined); return null;
        }
        case "pin": {
          if (!Number.isInteger(request.slot) || request.slot < 0 || request.slot > 3) throw new Error("Invalid pin slot");
          const command = buildPinnedJumpCommands().find(command => command.id === `pin-jump:${request.slot}`);
          if (!command) throw new Error("This pin is empty.");
          const pin = spotlightPins.get(request.slot);
          if (pin?.kind === "chat") await openDesktopWindow({ kind: "chat", workshopId: scope, resourceId: pin.target, title: pin.label });
          else await run(scope, command);
          return null;
        }
        case "activity": await focusShell(scope); await openActivityTarget(request.target, request.title, ctx()); return null;
        case "new-ask": await focusShell(scope); openNewAsk(); return null;
        case "connection": await focusShell(scope); ctx().openSettingsSection("basement"); return null;
        case "ssh": {
          const target = (await sshTargets()).find(target => target.target_id === request.targetId);
          requireScope(scope);
          if (!target) throw new Error("This SSH destination is no longer available. Refresh the picker.");
          await openDesktopWindow({ kind: "terminal", workshopId: scope, sshTargetId: target.target_id }); return null;
        }
        case "create-flow": {
          if (!Array.isArray(request.refs) || !request.refs.length || request.refs.length > 100) throw new Error("Choose up to 100 tool runs for this flow.");
          await focusShell(scope);
          const { promoteWorkflowFromSlice } = await import("$lib/daemon");
          const response = await promoteWorkflowFromSlice({ refs: request.refs, name: request.name.trim() || null, run: false });
          requireScope(scope);
          const { flows } = await import("$lib/stores/flows.svelte");
          const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
          requireScope(scope);
          flows.openComposer({ name: response.draft.name || request.name, steps: response.draft.steps });
          lmeWorkspace.focusFlowComposerTab(response.draft.name || request.name || "New flow"); return null;
        }
        default: throw new Error("Unknown Spotlight operation");
      }
    },
    dispose: () => { disposed = true; tickets.clear(); sourceRefresh = null; sshCache = null; },
  };
}

