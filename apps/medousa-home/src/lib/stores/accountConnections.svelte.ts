import { activeWorkshopId } from "$lib/utils/workshopLocality";
/**
 * Reactive account connections (ChatGPT / Cursor / Hermes) for Settings + chat runtime
 * gating. Probed via Tauri → daemon agents surface.
 */

import {
  probeAccountConnections,
  accountConnectionsSupported,
  type AccountConnectionInfo,
  type AccountConnections,
  type AccountAuthStatus,
  type AccountId,
} from "$lib/utils/accountConnections";

function createAccountConnectionsStore() {
  let connections = $state<AccountConnections | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let probedScope: string | null = null;
  let loadedScope: string | null = null;
  let sequence = 0;

  async function refresh(force = false) {
    if (!accountConnectionsSupported()) return;
    const scope = activeWorkshopId();
    if (loading && loadedScope === scope && !force) return;
    if (probedScope === scope && loadedScope === scope && connections && !force) return;
    const request = ++sequence;
    loadedScope = scope;
    connections = null;
    loading = true;
    error = null;
    try {
      const result = await probeAccountConnections();
      if (sequence !== request || activeWorkshopId() !== scope) return;
      connections = result;
      probedScope = scope;
    } catch (err) {
      if (sequence === request && activeWorkshopId() === scope) error = err instanceof Error ? err.message : String(err);
    } finally {
      if (sequence === request) loading = false;
    }
  }

  function connection(id: AccountId): AccountConnectionInfo | null {
    if (!connections || loadedScope !== activeWorkshopId()) return null;
    return connections[id];
  }

  function isSignedIn(id: AccountId): boolean {
    return connection(id)?.authStatus === "signed_in";
  }

  /**
   * Poll until the account reports signed_in (or timeout). Used after kicking
   * off vendor login in the browser / terminal so External Agents can flip status
   * without a manual refresh.
   */
  async function awaitSignedIn(
    id: AccountId,
    options?: { timeoutMs?: number; intervalMs?: number },
  ): Promise<AccountAuthStatus> {
    const timeoutMs = options?.timeoutMs ?? 3 * 60_000;
    const intervalMs = options?.intervalMs ?? 2_500;
    const scope = activeWorkshopId();
    const started = Date.now();
    while (Date.now() - started < timeoutMs && activeWorkshopId() === scope) {
      await refresh(true);
      const status = connection(id)?.authStatus ?? "unknown";
      if (status === "signed_in") return status;
      await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
    if (activeWorkshopId() !== scope) return "unknown";
    await refresh(true);
    return connection(id)?.authStatus ?? "unknown";
  }

  return {
    get connections() {
      return loadedScope === activeWorkshopId() ? connections : null;
    },
    get loading() {
      return loading;
    },
    get error() {
      return loadedScope === activeWorkshopId() ? error : null;
    },
    refresh,
    connection,
    isSignedIn,
    awaitSignedIn,
  };
}

export const accountConnections = createAccountConnectionsStore();
