import { bindAllFeaturePorts, unbindAllFeaturePorts } from "$lib/runtime/bindFeaturePorts";
import { setWorkshopSwitchPorts } from "$lib/runtime/workshopSwitchPorts";
import { activateWorkshopScope, connectWorkshop } from "$lib/workshopConnection";
import { workshops } from "$lib/stores/workshops.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { setActiveWorkshopKindPort } from "$lib/utils/workshopLocality";
import type { DaemonHealth } from "$lib/daemon";

/** A window owns its connection consumers, never the main window’s selection. */
export function connectDesktopWindow(scope: string | undefined, onReady: () => void | Promise<void>, onHealth: (health: DaemonHealth | null) => void, mode: "window" | "observer" = "window") {
  bindAllFeaturePorts();
  setActiveWorkshopKindPort(() => workshops.activeWorkshop?.kind);
  setWorkshopSwitchPorts({
    vaultDirty: () => false, flushVaultBeforeLeave: async () => true,
    prepareForWorkshopSwitch: async () => {}, activateWorkshopScope,
    hasLiveInteractiveTurn: () => chat.hasLiveInteractiveTurn(),
    chatSessionId: () => chat.sessionId,
    chatHasSession: id => chat.sessions.some(s => s.session_id === id),
    switchChatSession: id => chat.switchSession(id),
    setDaemonUrl: url => { settings.daemonUrl = url; },
    setColorTheme: (id, options) => settings.setColorTheme(id, options),
    applyTheme: () => settings.applyTheme(),
  });
  const stop = connectWorkshop({ mode, isolated: true, workshopId: scope, onHealthChange: onHealth, onReady });
  return () => { stop(); setWorkshopSwitchPorts(null); setActiveWorkshopKindPort(() => undefined); unbindAllFeaturePorts(); };
}
