import { invoke } from "@tauri-apps/api/core";
import type { ChannelId, ProductConfigSummary } from "$lib/types/messaging";
import {
  integrationSecretConfigured,
  patchIntegrationBaseUrl,
  resolveSecretTarget,
  upsertIntegrationSecret,
} from "$lib/integrationsClient";

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
  const target = resolveSecretTarget(secretId);
  if (target && "slot" in target) {
    try {
      return await integrationSecretConfigured(target.kind, target.slot);
    } catch {
      // Fall through to local status when the workshop HTTP path is unavailable.
    }
  }
  return invoke<boolean>("messaging_secret_status", { secretId });
}

export async function messagingSaveSecret(
  secretId: string,
  value: string | null,
): Promise<void> {
  const target = resolveSecretTarget(secretId);
  if (target) {
    try {
      if ("baseUrl" in target) {
        await patchIntegrationBaseUrl(target.kind, value);
        return;
      }
      await upsertIntegrationSecret(target.kind, target.slot, value);
      return;
    } catch {
      // Fall through to local typed store (co-located / onboard before socket).
    }
  }
  await invoke("messaging_save_secret", {
    secretId,
    value: value?.trim() ? value.trim() : null,
  });
}

export async function messagingClearSecret(secretId: string): Promise<void> {
  await messagingSaveSecret(secretId, null);
}

export async function messagingReadSecret(secretId: string): Promise<string | null> {
  // Secret values are never returned over daemon HTTP; local co-located read only.
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
