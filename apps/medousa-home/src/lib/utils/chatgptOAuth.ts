import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export type ChatGptOAuthStatus =
  | "signed_out"
  | "connected"
  | "refresh_required"
  | "reauth_required"
  | "plan_usage_disabled";

export interface ChatGptOAuthConnection {
  status: ChatGptOAuthStatus;
  connected: boolean;
  account_id?: string | null;
  expires_at_utc?: string | null;
  client_id?: string | null;
  email?: string | null;
  plan_usage_enabled?: boolean;
  profiles?: ChatGptAccountProfile[];
}

export interface ChatGptAccountProfile {
  client_id: string;
  account_id?: string | null;
  email?: string | null;
  connected: boolean;
  plan_usage_enabled: boolean;
}

export interface CompleteChatGptOAuthResponse {
  status: ChatGptOAuthStatus;
  first_connection: boolean;
  connection: ChatGptOAuthConnection;
}

export interface DisconnectChatGptOAuthResponse {
  disconnected: boolean;
  revoked: boolean;
}

export interface ChatGptModelListResponse {
  models: string[];
  display_names: Record<string, string>;
}

const SIGNED_OUT: ChatGptOAuthConnection = {
  status: "signed_out",
  connected: false,
};

async function chatGptOAuthRequest<T>(
  operation: "status" | "sign_in" | "select" | "refresh" | "disconnect" | "models",
  clientId?: string,
  enablePlanUsage = false,
): Promise<T> {
  const result = await invoke<T>("chatgpt_oauth_request", { operation, clientId: clientId ?? null, enablePlanUsage });
  if (["sign_in", "select", "disconnect", "refresh"].includes(operation) && typeof window !== "undefined") {
    window.dispatchEvent(new Event("medousa:chatgpt-account-changed"));
  }
  return result;
}

export async function getChatGptOAuthConnection(): Promise<ChatGptOAuthConnection> {
  if (!isTauri()) return SIGNED_OUT;
  return chatGptOAuthRequest<ChatGptOAuthConnection>("status");
}

export async function signInChatGptOAuth(clientId?: string, enablePlanUsage = false): Promise<CompleteChatGptOAuthResponse> {
  return chatGptOAuthRequest<CompleteChatGptOAuthResponse>("sign_in", clientId, enablePlanUsage);
}

export async function selectChatGptAccount(clientId: string): Promise<ChatGptOAuthConnection> {
  return chatGptOAuthRequest<ChatGptOAuthConnection>("select", clientId);
}

export const CHATGPT_USAGE_URL = "https://chatgpt.com/settings/usage";

export async function refreshChatGptOAuth(): Promise<ChatGptOAuthConnection> {
  return chatGptOAuthRequest<ChatGptOAuthConnection>("refresh");
}

export async function disconnectChatGptOAuth(): Promise<DisconnectChatGptOAuthResponse> {
  return chatGptOAuthRequest<DisconnectChatGptOAuthResponse>("disconnect");
}

export async function listChatGptOAuthModels(): Promise<ChatGptModelListResponse> {
  return chatGptOAuthRequest<ChatGptModelListResponse>("models");
}

export function chatGptOAuthReady(connection: ChatGptOAuthConnection | null): boolean {
  return connection?.plan_usage_enabled === true && (connection.status === "connected" || connection.status === "refresh_required");
}
