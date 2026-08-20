import { daemonUnary } from "$lib/daemon/contractClient";

export interface IntegrationSecretStatus {
  api_key?: boolean;
  oauth_bundle?: boolean;
  bot_token?: boolean;
  app_token?: boolean;
  auth_key?: boolean;
}

export interface IntegrationConnection {
  connection_id: string;
  kind: string;
  label?: string | null;
  base_url?: string | null;
  secrets: IntegrationSecretStatus;
  created_at: string;
  updated_at: string;
}

type SecretSlot = "api_key" | "oauth_bundle" | "bot_token" | "app_token" | "auth_key";

async function listConnections(): Promise<IntegrationConnection[]> {
  const response = await daemonUnary<{ connections: IntegrationConnection[] }>(
    "integrations.get",
  );
  return response.connections ?? [];
}

async function ensureConnection(kind: string): Promise<IntegrationConnection> {
  const existing = (await listConnections()).find((row) => row.kind === kind);
  if (existing) {
    return existing;
  }
  return daemonUnary<IntegrationConnection>("integrations.post", {}, { kind });
}

export async function upsertIntegrationSecret(
  kind: string,
  slot: SecretSlot,
  value: string | null,
): Promise<void> {
  const connection = await ensureConnection(kind);
  if (value && value.trim()) {
    await daemonUnary(
      "integrations.by_connection_id.secrets.by_slot.put",
      { connection_id: connection.connection_id, slot },
      { value: value.trim() },
    );
    return;
  }
  await daemonUnary("integrations.by_connection_id.secrets.by_slot.delete", {
    connection_id: connection.connection_id,
    slot,
  });
}

export async function patchIntegrationBaseUrl(
  kind: string,
  baseUrl: string | null,
): Promise<void> {
  const connection = await ensureConnection(kind);
  await daemonUnary(
    "integrations.by_connection_id.patch",
    { connection_id: connection.connection_id },
    { base_url: baseUrl?.trim() || null },
  );
}

export async function integrationSecretConfigured(
  kind: string,
  slot: SecretSlot,
): Promise<boolean> {
  const match = (await listConnections()).find((row) => row.kind === kind);
  if (!match) {
    return false;
  }
  return Boolean(match.secrets?.[slot]);
}

/** Map Home messaging secret ids onto integration kind + slot. */
export function resolveSecretTarget(
  secretId: string,
): { kind: string; slot: SecretSlot } | { kind: string; baseUrl: true } | null {
  switch (secretId) {
    case "telegram_bot_token":
      return { kind: "telegram", slot: "bot_token" };
    case "discord_bot_token":
      return { kind: "discord", slot: "bot_token" };
    case "slack_bot_token":
      return { kind: "slack", slot: "bot_token" };
    case "slack_app_token":
      return { kind: "slack", slot: "app_token" };
    case "api_key":
      return { kind: "openai", slot: "api_key" };
    case "stt_api_key":
      return { kind: "stt.openai", slot: "api_key" };
    default:
      break;
  }
  if (secretId.startsWith("api_key_")) {
    return { kind: secretId.slice("api_key_".length), slot: "api_key" };
  }
  if (secretId.startsWith("base_url_")) {
    return { kind: secretId.slice("base_url_".length), baseUrl: true };
  }
  return null;
}
