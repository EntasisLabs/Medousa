import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export type ChatGptOAuthStatus =
  | "signed_out"
  | "connected"
  | "refresh_required"
  | "reauth_required";

export interface ChatGptOAuthConnection {
  status: ChatGptOAuthStatus;
  connected: boolean;
  account_id?: string | null;
  expires_at_utc?: string | null;
}

export interface BeginChatGptOAuthResponse {
  login_id: string;
  verification_url: string;
  user_code: string;
  expires_at_utc: string;
  poll_interval_seconds: number;
}

export interface CompleteChatGptOAuthResponse {
  status: "pending" | "connected";
  retry_after_seconds?: number | null;
  connection?: ChatGptOAuthConnection | null;
}

export interface DisconnectChatGptOAuthResponse {
  disconnected: boolean;
  revoked: boolean;
}

export interface ChatGptModelListResponse {
  models: string[];
}

const SIGNED_OUT: ChatGptOAuthConnection = {
  status: "signed_out",
  connected: false,
};

async function chatGptOAuthRequest<T>(
  operation: "status" | "begin" | "complete" | "refresh" | "disconnect" | "models",
  loginId?: string,
): Promise<T> {
  return invoke<T>("chatgpt_oauth_request", { operation, loginId: loginId ?? null });
}

export async function getChatGptOAuthConnection(): Promise<ChatGptOAuthConnection> {
  if (!isTauri()) return SIGNED_OUT;
  return chatGptOAuthRequest<ChatGptOAuthConnection>("status");
}

export async function beginChatGptOAuth(): Promise<BeginChatGptOAuthResponse> {
  return chatGptOAuthRequest<BeginChatGptOAuthResponse>("begin");
}

export async function completeChatGptOAuth(
  loginId: string,
): Promise<CompleteChatGptOAuthResponse> {
  return chatGptOAuthRequest<CompleteChatGptOAuthResponse>("complete", loginId);
}

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
  return connection?.status === "connected" || connection?.status === "refresh_required";
}
