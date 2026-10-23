import type { WorkshopCommand } from "$lib/commands/types";
import type { SessionSummary } from "$lib/types/session";
import type { CreativeProjectSummary } from "$lib/types/generated/daemon_api";
import type { ItemProjection } from "$lib/forge";
import type { DesktopTarget } from "./windows";

/** The same Spotlight command/search UI, with native resource destinations. */
export function desktopWindowCommands(
  scope: string,
  sessions: SessionSummary[],
  canvases: CreativeProjectSummary[],
  projects: ItemProjection[],
  open: (target: DesktopTarget) => Promise<unknown>,
  newChat: () => Promise<unknown>,
  main: () => Promise<unknown>,
): WorkshopCommand[] {
  const destination = (id: string, label: string, target: DesktopTarget, subtitle: string): WorkshopCommand => ({
    id, section: "open", label, subtitle, keywords: `${target.kind} ${label}`,
    run: async (ctx) => { await open(target); ctx.callbacks.close(); },
  });
  return [
    { id: "desktop-new-chat", section: "suggested", label: "New chat", subtitle: "Start a conversation in its own window", keywords: "ask chat new conversation", run: async ctx => { await newChat(); ctx.callbacks.close(); } },
    ...(["chat", "create", "code"] as const).map(kind => destination(`desktop-${kind}`, kind === "chat" ? "Chat" : kind === "create" ? "Create" : "Code", { kind, workshopId: scope }, kind === "chat" ? "Browse conversations" : kind === "create" ? "Browse canvases or start something new" : "Browse code projects")),
    ...sessions.map(session => ({
      ...destination(`desktop-chat:${session.session_id}`, session.display_name?.trim() || session.preview?.slice(0, 80) || "Conversation", { kind: "chat", workshopId: scope, resourceId: session.session_id, title: session.display_name }, "Chat · conversation"),
      preview: { kind: "chat" as const, sessionId: session.session_id, text: session.preview },
    })),
    ...canvases.map(canvas => destination(`desktop-create:${canvas.id}`, canvas.title, { kind: "create", workshopId: scope, resourceId: canvas.id, title: canvas.title }, "Create · canvas")),
    ...projects.map(project => destination(`desktop-code:${project.id}`, project.title, { kind: "code", workshopId: scope, resourceId: project.id, title: project.title }, "Code · project")),
    { id: "desktop-main", section: "go", label: "Open Medousa", subtitle: "Your full workspace and tabs", keywords: "main home settings workspace", run: async ctx => { await main(); ctx.callbacks.close(); } },
  ];
}
