import { EXTERNAL_AGENTS, externalAgent, type ExternalAgentRuntime, type ExternalAgentAccount } from "$lib/utils/externalAgentCatalog";
import { daemonUnary } from "$lib/daemon/contractClient";
import type { AgentRuntimeListResponse } from "$lib/daemon/session";
import { activeWorkshopId, isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
/**
 * Settings → External Agents (Codex / Cursor / Hermes) — Tauri bridge for vendor CLI
 * login orchestration. Auth status comes from the daemon agents surface.
 */

import { invoke } from "@tauri-apps/api/core";
import { isTauriDesktop } from "$lib/platform";

export type AccountAuthStatus = "signed_in" | "signed_out" | "unknown";

export type AccountId = ExternalAgentAccount;

export interface AccountConnectionInfo {
  id: string;
  label: string;
  runtime: string;
  binaryPresent: boolean;
  available?: boolean;
  command: string | null;
  authStatus: AccountAuthStatus;
  detail: string | null;
}

export type AccountConnections = Record<AccountId, AccountConnectionInfo>;

export interface DeviceAuthStart {
  url: string;
  code: string | null;
  detail: string | null;
}

export function accountConnectionsSupported(): boolean {
  return true;
}

export async function probeAccountConnections(): Promise<AccountConnections> {
  const response = await daemonUnary<AgentRuntimeListResponse>("agents.runtimes.get", {}, undefined, undefined, undefined, activeWorkshopId());
  return Object.fromEntries(EXTERNAL_AGENTS.map(agent => {
    const info = response.runtimes.find(runtime => runtime.runtime === agent.id);
    return [agent.account, {
      id: agent.account, label: agent.label, runtime: agent.id,
      binaryPresent: info?.binary_present ?? info?.available ?? false,
      available: info?.available ?? false,
      command: info?.command ?? null,
      authStatus: info?.auth_status === "signed_in" || info?.auth_status === "signed_out" ? info.auth_status : "unknown",
      detail: info?.auth_detail ?? info?.detail ?? "This workshop does not report this runtime.",
    } satisfies AccountConnectionInfo];
  })) as AccountConnections;
}

function requireLocalSetup() {
  if (!isTauriDesktop() || !isCoLocatedWorkshop()) throw new Error("Configure external agents on the workshop host. This device cannot change that host’s credentials.");
}

export async function beginChatgptDeviceLogin(): Promise<DeviceAuthStart> {
  requireLocalSetup();
  return invoke<DeviceAuthStart>("account_chatgpt_begin_device_login");
}

export async function beginTerminalLogin(account: AccountId): Promise<string> {
  requireLocalSetup();
  return invoke<string>("account_begin_terminal_login", { account });
}

export async function accountSignOut(account: AccountId): Promise<string> {
  requireLocalSetup();
  return invoke<string>("account_sign_out", { account });
}

export interface AccountCliInstallResult {
  account: string;
  command: string;
  detail: string;
}

/** Install Codex / Cursor / Hermes CLI via the vendor's official installer. */
export async function installAccountCli(
  account: AccountId,
): Promise<AccountCliInstallResult> {
  requireLocalSetup();
  return invoke<AccountCliInstallResult>("account_cli_install", { account });
}

export function authStatusLabel(status: AccountAuthStatus): string {
  switch (status) {
    case "signed_in":
      return "Signed in";
    case "signed_out":
      return "Signed out";
    default:
      return "Unknown";
  }
}

/** Map chat runtime id → External Agents account card id. */
export function accountIdForRuntime(
  runtime: ExternalAgentRuntime,
): AccountId {
  return externalAgent(runtime).account;
}
