/**
 * Turn-stream side effects that are not transcript mutations: permissions,
 * Agent Browser challenge/navigation, and budget-approval alerts.
 */

import type {
  InteractiveTurnStreamEvent,
  PendingBudgetApproval,
} from "$lib/types/chat";
import { chatSettingsPort } from "$lib/runtime/chatSettingsPort";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";

export function clearBudgetAlert(host: ChatStoreHost) {
  host.budgetAlert = null;
}

export function clearPermissionAlert(host: ChatStoreHost) {
  host.permissionAlert = null;
}

export function notePermissionResolved(host: ChatStoreHost, requestId: string) {
  if (host.permissionAlert?.requestId === requestId) {
    host.permissionAlert = null;
  }
}

export function handlePermissionRequest(host: ChatStoreHost, event: InteractiveTurnStreamEvent) {
  const requestId = event.permission_request_id?.trim();
  if (!requestId) return;
  host.permissionAlert = {
    turnId: event.turn_id,
    messageId: host.messageIdForTurn(event.turn_id),
    requestId,
    agentSessionId: event.agent_session_id?.trim() || null,
    agentRuntime: event.agent_runtime?.trim() || null,
    message:
      event.operator_message?.trim() ||
      event.message?.trim() ||
      "Agent needs permission to continue",
  };
}

export function clearSecretAlert(host: ChatStoreHost) {
  host.secretAlert = null;
}

export function handleSecretRequest(host: ChatStoreHost, event: InteractiveTurnStreamEvent) {
  const requestId = event.secret_request_id?.trim();
  const label = event.secret_label?.trim();
  const providerType = event.secret_provider_type?.trim();
  const credentialKey = event.secret_credential_key?.trim();
  const backend =
    event.secret_backend === "grapheme_runtime" ? "grapheme_runtime" : "openshell_provider";
  if (!requestId || !label || !providerType || !credentialKey) return;
  host.secretAlert = {
    turnId: event.turn_id,
    messageId: host.messageIdForTurn(event.turn_id),
    requestId,
    label,
    reason:
      event.operator_message?.trim() ||
      event.message?.trim() ||
      "The sandbox needs a credential to continue",
    providerType,
    credentialKey,
    backend,
    allowedHosts: event.secret_allowed_hosts ?? [],
  };
}

export function clearBrowserChallenge(host: ChatStoreHost, sessionId?: string) {
  if (!sessionId || host.browserChallenge?.sessionId === sessionId) {
    host.browserChallenge = null;
  }
}

export function handleBrowserChallenge(host: ChatStoreHost, event: InteractiveTurnStreamEvent) {
  const sessionId = event.browser_session_id?.trim();
  if (!sessionId) return;
  const challengeUrl = event.browser_challenge_url?.trim() || null;
  const isClientAct = !challengeUrl;
  const messageId = host.messageIdForTurn(event.turn_id);

  if (event.message === "client_search") {
    void executeClientBrowserSearch(host, event, sessionId);
    return;
  }

  if (isClientAct) {
    void executeClientBrowserAct(sessionId);
    return;
  }

  host.browserChallenge = {
    turnId: event.turn_id,
    messageId,
    sessionId,
    challengeUrl,
    message: event.message || event.operator_message || "",
  };
  const workCardId = host.workCardIdForTurn(event.turn_id);
  void import("$lib/stores/browser.svelte").then(({ browser }) =>
    browser.setControl("awaiting_operator"),
  );
  if (challengeUrl) {
    void import("$lib/utils/openInBrowser").then(({ openInBrowser }) =>
      openInBrowser(challengeUrl, {
        openedBy: "agent",
        sessionId: host.sessionId,
        workCardId,
      }),
    );
  }
}

const runningClientSearches = new Map<string, string>();

async function executeClientBrowserSearch(
  host: ChatStoreHost,
  event: InteractiveTurnStreamEvent,
  sessionId: string,
) {
  if (runningClientSearches.has(sessionId)) return;
  runningClientSearches.set(sessionId, event.turn_id);
  let worldDriverId: string | null | undefined;
  try {
    const { fetchBrowserSession, completeBrowserSession } = await import("$lib/daemon");
    const session = await fetchBrowserSession(sessionId);
    worldDriverId = session.world_driver_id;
    const { openInBrowser } = await import("$lib/utils/openInBrowser");
    const { humanBrowserSnapshotSearch } = await import("$lib/humanBrowser");
    const { humanBrowser } = await import("$lib/stores/humanBrowser.svelte");
    const { governedBrowser } = await import("$lib/stores/governedBrowser.svelte");
    // This request was explicitly admitted to the device driver. Show that
    // source, not an unrelated workshop-world frame hiding the native page.
    governedBrowser.chooseDevice();
    await openInBrowser(event.browser_challenge_url || `https://html.duckduckgo.com/html/?q=${encodeURIComponent(session.query)}`, {
      openedBy: "agent",
      sessionId: host.sessionId,
      workCardId: host.workCardIdForTurn(event.turn_id),
      title: `Search: ${session.query}`,
    });

    // Navigation completes asynchronously in the native webview. Empty early
    // snapshots are not search failures; wait for results or a real challenge.
    const deadline = Date.now() + 20_000;
    do {
      await new Promise((resolve) => setTimeout(resolve, 500));
      if (humanBrowser.loading) continue;
      const response = await humanBrowserSnapshotSearch(session.query, session.max_results);
      if (response.challenge && response.challenge !== "empty_results") {
        host.browserChallenge = {
          turnId: event.turn_id,
          messageId: host.messageIdForTurn(event.turn_id),
          sessionId,
          challengeUrl: event.browser_challenge_url || `https://html.duckduckgo.com/html/?q=${encodeURIComponent(session.query)}`,
          message: "Complete the web verification in this tab, then tap Continue agent.",
        };
        const { browser } = await import("$lib/stores/browser.svelte");
        await browser.setControl("awaiting_operator");
        // Keep the loaded CAPTCHA intact; reopening it can reset verification.
        return;
      }
      if (response.results.length) {
        await completeBrowserSession(sessionId, { worldDriverId, searchResponse: response });
        clearBrowserChallenge(host, sessionId);
        return;
      }
    } while (Date.now() < deadline);
    throw new Error("The search page loaded without readable results. Try the search again.");
  } catch (error) {
    try {
      const { completeBrowserSession } = await import("$lib/daemon");
      await completeBrowserSession(sessionId, {
        worldDriverId,
        error: error instanceof Error ? error.message : String(error),
      });
    } catch {
      // The turn may have ended or its workshop disconnected.
    }
  } finally {
    runningClientSearches.delete(sessionId);
  }
}

async function executeClientBrowserAct(sessionId: string) {
  let worldDriverId: string | null | undefined;
  try {
    const { fetchBrowserSession, completeBrowserActSession } = await import("$lib/daemon");
    const session = await fetchBrowserSession(sessionId);
    worldDriverId = session.world_driver_id;
    const request = session.act_request;
    if (!request) {
      await completeBrowserActSession(sessionId, {
        worldDriverId,
        ok: false,
        error: "act session missing request payload",
      });
      return;
    }
    const { humanBrowserAct } = await import("$lib/humanBrowser");
    const report = await humanBrowserAct(request);
    const actionSummary = [request.action, request.selector]
      .filter((part): part is string => Boolean(part?.trim()))
      .join(" ");
    void import("$lib/stores/browser.svelte").then(({ browser }) =>
      browser.noteAgentActivity(
        report.ok
          ? `Medousa ${actionSummary}`.trim()
          : `Act failed: ${report.error ?? actionSummary}`.trim(),
      ),
    );
    await completeBrowserActSession(sessionId, {
      worldDriverId,
      ok: report.ok,
      url: report.url,
      error: report.error ?? null,
    });
  } catch (err) {
    try {
      const { completeBrowserActSession } = await import("$lib/daemon");
      await completeBrowserActSession(sessionId, {
        worldDriverId,
        ok: false,
        error: err instanceof Error ? err.message : String(err),
      });
    } catch {
      // Session may have expired — nothing else to do.
    }
  }
}

export function handleBrowserNavigated(host: ChatStoreHost, event: InteractiveTurnStreamEvent) {
  // The client-search handler already owns opening and reading this page.
  if ([...runningClientSearches.values()].includes(event.turn_id)) return;
  if (!chatSettingsPort().autoOpenWebOnAgentBrowse()) return;
  const url = event.message?.trim();
  if (!url) return;
  const workCardId = host.workCardIdForTurn(event.turn_id);
  void import("$lib/utils/openInBrowser").then(({ openInBrowser }) =>
    openInBrowser(url, {
      openedBy: "agent",
      sessionId: host.sessionId,
      workCardId,
      title: event.operator_message ?? undefined,
    }),
  );
}

export function pendingBudgetApprovals(host: ChatStoreHost): PendingBudgetApproval[] {
  const items: PendingBudgetApproval[] = [];
  for (const [turnId, turn] of host.turns) {
    if (turn.terminal) continue;
    if (
      turn.phase !== "budget_blocked" &&
      turn.phase !== "budget_approval" &&
      !turn.budgetRequestId
    ) {
      continue;
    }
    const requestId = turn.budgetRequestId?.trim();
    if (!requestId) continue;
    items.push({
      turnId,
      messageId: turn.messageId,
      requestId,
      workCardId: turn.workspaceCardId?.trim() || requestId,
      requestedRounds: turn.requestedRounds ?? null,
      message: "Medousa needs more tool rounds to finish this task.",
    });
  }
  return items;
}

export function hasPendingBudgetApproval(host: ChatStoreHost, requestId: string): boolean {
  const id = requestId.trim();
  if (!id) return false;
  if (host.budgetAlert?.requestId === id) return true;
  return pendingBudgetApprovals(host).some((item) => item.requestId === id);
}

export function noteBudgetResolved(host: ChatStoreHost, requestId: string) {
  if (host.budgetAlert?.requestId === requestId) {
    host.budgetAlert = null;
  }
  const next = new Map(host.turns);
  for (const [turnId, turn] of next) {
    if (turn.budgetRequestId === requestId) {
      next.set(turnId, {
        ...turn,
        phase: "tool_loop",
        budgetRequestId: null,
        requestedRounds: null,
      });
    }
  }
  host.turns = next;
  if (host.backgroundActivity > 0) {
    host.backgroundActivity -= 1;
  }
}
