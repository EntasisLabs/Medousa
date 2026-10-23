import { publishChatWindowEvent } from "$lib/chat/chatWindowSync";
import { listen } from "@tauri-apps/api/event";
import { openDaemonEventStream, type DaemonEventConnection } from "$lib/daemon/daemonEventStream";
import { chat } from "$lib/stores/chat.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { environment } from "$lib/stores/environment.svelte";
import { connection } from "$lib/stores/connection.svelte";
import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
import type { WorkspaceStreamEvent } from "$lib/types/workspace";
import type { EnvironmentStreamEvent } from "$lib/types/environment";

/** Independent, scoped consumers. A main-window workshop switch cannot cancel
 * these streams or deliver another workshop's snapshots into their projections. */
export async function startDesktopStreams(): Promise<() => void> {
  let disposed = false;
  const stops: (() => void)[] = [];
  let workspaceSource: DaemonEventConnection | null = null, environmentSource: DaemonEventConnection | null = null;
  const timers = new Set<ReturnType<typeof setTimeout>>();
  const later = (run: () => void) => {
    if (disposed) return;
    const timer = setTimeout(() => { timers.delete(timer); if (!disposed) run(); }, 2000);
    timers.add(timer);
  };
  stops.push(...await Promise.all([
    listen<TurnStreamEnvelopeV3>("desktop-interactive://event", event => { if (!disposed) { connection.noteTraffic(); chat.applyStreamEvent(event.payload); publishChatWindowEvent(event.payload); } }),
    listen<{ message: string }>("desktop-interactive://error", event => { if (!disposed) { chat.noteStreamFailure(event.payload.message, { recoverable: true }); later(() => void chat.tryReattachActiveTurn()); } }),
  ]));

  function workspaceStream() {
    void openDaemonEventStream<WorkspaceStreamEvent>({
      operation: "workspace.stream.get", query: workspace.revision ? { since_revision: String(workspace.revision) } : {},
      browserUrl: async () => "", browserEvent: "workspace",
      onEvent: event => { if (!disposed) { connection.noteTraffic(); workspace.applyEvent(event); } },
      onError: () => later(workspaceStream),
    }).then(source => disposed ? source.close() : (workspaceSource = source)).catch(() => later(workspaceStream));
  }
  function environmentStream() {
    void openDaemonEventStream<EnvironmentStreamEvent>({
      operation: "environment.spec.stream.get", browserUrl: async () => "", browserEvent: "environment",
      onEvent: event => { if (!disposed) environment.applyEvent(event); },
      onError: () => later(environmentStream),
    }).then(source => disposed ? source.close() : (environmentSource = source)).catch(() => later(environmentStream));
  }
  workspaceStream(); environmentStream();
  return () => { disposed = true; stops.forEach(stop => stop()); workspaceSource?.close(); environmentSource?.close(); timers.forEach(clearTimeout); };
}
