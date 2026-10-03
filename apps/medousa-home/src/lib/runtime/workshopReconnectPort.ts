/** Port so the workshops store does not import workshopConnection. */

import type { DaemonHealth } from "$lib/daemon";

export type WorkshopReconnectFn = (
  onHealthChange?: (health: DaemonHealth | null) => void,
) => Promise<DaemonHealth | null>;

let reconnectPort: WorkshopReconnectFn | null = null;
let refreshPort: WorkshopReconnectFn | null = null;

export function setWorkshopRefreshPort(port: WorkshopReconnectFn | null): void {
  refreshPort = port;
}

export async function requestWorkshopRefresh(
  onHealthChange?: (health: DaemonHealth | null) => void,
): Promise<DaemonHealth | null> {
  if (!refreshPort) throw new Error("The workshop connection is still starting. Try again in a moment.");
  return refreshPort(onHealthChange);
}

export function setWorkshopReconnectPort(port: WorkshopReconnectFn | null): void {
  reconnectPort = port;
}

export async function requestWorkshopReconnect(
  onHealthChange?: (health: DaemonHealth | null) => void,
): Promise<DaemonHealth | null> {
  if (!reconnectPort) return null;
  return reconnectPort(onHealthChange);
}
