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
  let probedOnce = false;

  async function refresh(force = false) {
    if (!accountConnectionsSupported()) return;
    if (loading && !force) return;
    if (probedOnce && !force) return;
    loading = true;
    error = null;
    try {
      connections = await probeAccountConnections();
      probedOnce = true;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function connection(id: AccountId): AccountConnectionInfo | null {
    if (!connections) return null;
    return connections[id];
  }

  function isSignedIn(id: AccountId): boolean {
    return connection(id)?.authStatus === "signed_in";
  }

  /**
   * Poll until the account reports signed_in (or timeout). Used after kicking
   * off vendor login in the browser / terminal so Connections can flip status
   * without a manual refresh.
   */
  async function awaitSignedIn(
    id: AccountId,
    options?: { timeoutMs?: number; intervalMs?: number },
  ): Promise<AccountAuthStatus> {
    const timeoutMs = options?.timeoutMs ?? 3 * 60_000;
    const intervalMs = options?.intervalMs ?? 2_500;
    const started = Date.now();
    while (Date.now() - started < timeoutMs) {
      await refresh(true);
      const status = connection(id)?.authStatus ?? "unknown";
      if (status === "signed_in") return status;
      await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
    await refresh(true);
    return connection(id)?.authStatus ?? "unknown";
  }

  return {
    get connections() {
      return connections;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    refresh,
    connection,
    isSignedIn,
    awaitSignedIn,
  };
}

export const accountConnections = createAccountConnectionsStore();
