/** Port so the workshops store does not import workshopConnection. */

import type { DaemonHealth } from "$lib/daemon";

export type WorkshopReconnectFn = (
  onHealthChange?: (health: DaemonHealth | null) => void,
) => Promise<DaemonHealth | null>;

let reconnectPort: WorkshopReconnectFn | null = null;

export function setWorkshopReconnectPort(port: WorkshopReconnectFn | null): void {
  reconnectPort = port;
}

export async function requestWorkshopReconnect(
  onHealthChange?: (health: DaemonHealth | null) => void,
): Promise<DaemonHealth | null> {
  if (!reconnectPort) return null;
  return reconnectPort(onHealthChange);
}
