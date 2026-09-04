import { invoke } from "@tauri-apps/api/core";
import type {
  BotListResponse,
  BotOpenResponse,
  BotProfile,
  CreateBotRequest,
  DuplicateBotRequest,
  SessionBotResponse,
  SetBotArchivedRequest,
  SetSessionBotRequest,
  UpdateBotRequest,
} from "$lib/types/generated/daemon_api";

export async function listBots(): Promise<BotListResponse> {
  return invoke<BotListResponse>("bot_list");
}

export async function createBot(request: CreateBotRequest): Promise<BotOpenResponse> {
  return invoke<BotOpenResponse>("bot_create", { request });
}

export async function getBot(botId: string): Promise<BotProfile> {
  return invoke<BotProfile>("bot_get", { botId });
}

export async function updateBot(
  botId: string,
  request: UpdateBotRequest,
): Promise<BotProfile> {
  return invoke<BotProfile>("bot_update", { botId, request });
}

export async function setBotArchived(
  botId: string,
  request: SetBotArchivedRequest,
): Promise<BotProfile> {
  return invoke<BotProfile>("bot_set_archived", { botId, request });
}

export async function duplicateBot(
  botId: string,
  request: DuplicateBotRequest = {},
): Promise<BotOpenResponse> {
  return invoke<BotOpenResponse>("bot_duplicate", { botId, request });
}

export async function openBot(botId: string): Promise<BotOpenResponse> {
  return invoke<BotOpenResponse>("bot_open", { botId });
}

export async function getSessionBot(sessionId: string): Promise<SessionBotResponse> {
  return invoke<SessionBotResponse>("session_get_bot", { sessionId });
}

export async function setSessionBot(
  sessionId: string,
  request: SetSessionBotRequest,
): Promise<SessionBotResponse> {
  return invoke<SessionBotResponse>("session_set_bot", { sessionId, request });
}

export async function clearSessionBot(sessionId: string): Promise<SessionBotResponse> {
  return invoke<SessionBotResponse>("session_clear_bot", { sessionId });
}
