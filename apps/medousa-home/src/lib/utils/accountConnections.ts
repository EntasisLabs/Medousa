/**
 * Settings → Connections (ChatGPT / Cursor / Hermes) — Tauri bridge for vendor CLI
 * login orchestration. Auth status comes from the daemon agents surface.
 */

import { invoke } from "@tauri-apps/api/core";
import { isTauriDesktop } from "$lib/platform";

export type AccountAuthStatus = "signed_in" | "signed_out" | "unknown";

export type AccountId = "chatgpt" | "cursor" | "hermes";

export interface AccountConnectionInfo {
  id: string;
  label: string;
  runtime: string;
  binaryPresent: boolean;
  command: string | null;
  authStatus: AccountAuthStatus;
  detail: string | null;
}

export interface AccountConnections {
  chatgpt: AccountConnectionInfo;
  cursor: AccountConnectionInfo;
  hermes: AccountConnectionInfo;
}

export interface DeviceAuthStart {
  url: string;
  code: string | null;
  detail: string | null;
}

export function accountConnectionsSupported(): boolean {
  return isTauriDesktop();
}

export async function probeAccountConnections(): Promise<AccountConnections> {
  return invoke<AccountConnections>("account_connections_probe");
}

export async function beginChatgptDeviceLogin(): Promise<DeviceAuthStart> {
  return invoke<DeviceAuthStart>("account_chatgpt_begin_device_login");
}

export async function beginTerminalLogin(account: AccountId): Promise<string> {
  return invoke<string>("account_begin_terminal_login", { account });
}

export async function accountSignOut(account: AccountId): Promise<string> {
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

/** Map chat runtime id → Connections account card id. */
export function accountIdForRuntime(
  runtime: "cursor" | "codex" | "hermes",
): AccountId {
  if (runtime === "codex") return "chatgpt";
  return runtime;
}
