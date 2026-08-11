import { getDaemonUrl } from "$lib/daemon";
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

async function chatGptOAuthRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  const response = await fetch(`${base}${path}`, init);
  if (!response.ok) {
    const detail = await response.text().catch(() => "");
    throw new Error(detail || `ChatGPT account request failed (${response.status})`);
  }
  return response.json() as Promise<T>;
}

export async function getChatGptOAuthConnection(): Promise<ChatGptOAuthConnection> {
  if (!isTauri()) return SIGNED_OUT;
  return chatGptOAuthRequest<ChatGptOAuthConnection>("/v1/auth/chatgpt");
}

export async function beginChatGptOAuth(): Promise<BeginChatGptOAuthResponse> {
  return chatGptOAuthRequest<BeginChatGptOAuthResponse>("/v1/auth/chatgpt/begin", {
    method: "POST",
  });
}

export async function completeChatGptOAuth(
  loginId: string,
): Promise<CompleteChatGptOAuthResponse> {
  return chatGptOAuthRequest<CompleteChatGptOAuthResponse>("/v1/auth/chatgpt/complete", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ login_id: loginId }),
  });
}

export async function refreshChatGptOAuth(): Promise<ChatGptOAuthConnection> {
  return chatGptOAuthRequest<ChatGptOAuthConnection>("/v1/auth/chatgpt/refresh", {
    method: "POST",
  });
}

export async function disconnectChatGptOAuth(): Promise<DisconnectChatGptOAuthResponse> {
  return chatGptOAuthRequest<DisconnectChatGptOAuthResponse>("/v1/auth/chatgpt", {
    method: "DELETE",
  });
}

export async function listChatGptOAuthModels(): Promise<ChatGptModelListResponse> {
  return chatGptOAuthRequest<ChatGptModelListResponse>("/v1/auth/chatgpt/models");
}

export function chatGptOAuthReady(connection: ChatGptOAuthConnection | null): boolean {
  return connection?.status === "connected" || connection?.status === "refresh_required";
}
