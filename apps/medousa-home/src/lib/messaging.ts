import { invoke } from "@tauri-apps/api/core";
import type { ChannelId, ProductConfigSummary } from "$lib/types/messaging";

export async function loadProductConfigSummary(): Promise<ProductConfigSummary> {
  return invoke<ProductConfigSummary>("messaging_load_product_config_summary");
}

export async function saveTelegramConfig(config: {
  allowedUserIds: number[];
  heartbeatNudgesEnabled: boolean;
  heartbeatChatIds: number[];
}): Promise<void> {
  await invoke("messaging_save_channel_config", {
    request: { channel: "telegram", ...config },
  });
}

export async function saveDiscordConfig(config: {
  commandPrefix: string;
  heartbeatNudgesEnabled: boolean;
  heartbeatChannelIds: number[];
}): Promise<void> {
  await invoke("messaging_save_channel_config", {
    request: { channel: "discord", ...config },
  });
}

export async function saveSlackConfig(config: {
  allowedUserIds: string[];
  heartbeatNudgesEnabled: boolean;
  heartbeatChannelIds: string[];
}): Promise<void> {
  await invoke("messaging_save_channel_config", {
    request: { channel: "slack", ...config },
  });
}

export async function saveWhatsAppConfig(config: {
  deliverBind: string;
  deliverUrl?: string | null;
  sessionDbPath?: string | null;
  allowedUserIds: string[];
  heartbeatNudgesEnabled: boolean;
  heartbeatChatJids: string[];
}): Promise<void> {
  await invoke("messaging_save_channel_config", {
    request: { channel: "whatsapp", ...config },
  });
}

export async function messagingSecretStatus(secretId: string): Promise<boolean> {
  return invoke<boolean>("messaging_secret_status", { secretId });
}

export async function messagingSaveSecret(
  secretId: string,
  value: string | null,
): Promise<void> {
  await invoke("messaging_save_secret", {
    secretId,
    value: value?.trim() ? value.trim() : null,
  });
}

export async function messagingClearSecret(secretId: string): Promise<void> {
  await invoke("messaging_clear_secret", { secretId });
}

export async function messagingReadSecret(secretId: string): Promise<string | null> {
  return invoke<string | null>("messaging_read_secret", { secretId });
}

export function parseNumberCsv(raw: string): number[] {
  return raw
    .split(",")
    .map((token) => token.trim())
    .filter(Boolean)
    .map((token) => Number(token))
    .filter((value) => Number.isFinite(value));
}

export function parseStringCsv(raw: string): string[] {
  return raw
    .split(",")
    .map((token) => token.trim())
    .filter(Boolean);
}

export function formatNumberCsv(values: number[]): string {
  return values.join(", ");
}

export function formatStringCsv(values: string[]): string {
  return values.join(", ");
}

export type { ChannelId };
