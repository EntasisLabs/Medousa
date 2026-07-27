/**
 * P0 Trust — ensure the local workshop engine is up before streams/UI assume health.
 * Remote workshops no-op inside daemon_start / ensure_local_engine.
 */

import { checkDaemonHealth, type DaemonHealth } from "$lib/daemon";
import { isTauriMobilePlatform } from "$lib/platform";
import { isTauri } from "$lib/window";
import { startEngine, waitForEngine } from "$lib/utils/providersApi";

export type EnsureWorkshopEngineOptions = {
  /** Skip spawn (e.g. observer popouts that don't own the engine). */
  allowSpawn?: boolean;
  timeoutSeconds?: number;
};

/**
 * If health is already ok, return it. On Tauri desktop, try start + wait when down.
 * Mobile / browser: probe only (paired remote or no sidecar).
 */
export async function ensureWorkshopEngineHealthy(
  options?: EnsureWorkshopEngineOptions,
): Promise<DaemonHealth> {
  const allowSpawn = options?.allowSpawn ?? true;
  let health = await checkDaemonHealth();
  if (health.ok) return health;

  const canSpawn =
    allowSpawn && isTauri() && !isTauriMobilePlatform();
  if (!canSpawn) return health;

  try {
    await startEngine({ privateBrain: false });
    const wait = await waitForEngine(options?.timeoutSeconds ?? 45);
    health = await checkDaemonHealth();
    if (!health.ok && wait.message) {
      return { ...health, message: wait.message || health.message };
    }
    return health;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      ok: false,
      message: message || health.message || "Couldn’t start Medousa engine",
    };
  }
}
