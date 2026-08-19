import { daemonUnary } from "$lib/daemon";
import { CUSTOM_PROVIDER_CATALOG_ID } from "$lib/utils/customProvider";

export type IntegrationSecretSlot =
  | "api_key"
  | "oauth_bundle"
  | "bot_token"
  | "app_token"
  | "auth_key";

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
  label: string;
  catalog_id?: string | null;
  base_url?: string | null;
  secrets: IntegrationSecretStatus;
  created_at_utc: string;
  updated_at_utc: string;
}

interface IntegrationListResponse {
  connections: IntegrationConnection[];
}

interface IntegrationSecretMutationResponse {
  connection_id: string;
  slot: IntegrationSecretSlot;
  configured: boolean;
}

export async function listIntegrations(): Promise<IntegrationConnection[]> {
  const response = await daemonUnary<IntegrationListResponse>("integrations.get");
  return response.connections ?? [];
}

export async function findConnectionByKind(
  kind: string,
): Promise<IntegrationConnection | null> {
  const wanted = kind.trim().toLowerCase();
  if (!wanted) return null;
  const matches = (await listIntegrations()).filter(
    (row) => row.kind.trim().toLowerCase() === wanted,
  );
  return matches[0] ?? null;
}

export async function findCustomConnection(): Promise<IntegrationConnection | null> {
  const matches = (await listIntegrations()).filter(
    (row) =>
      row.catalog_id?.trim().toLowerCase() === CUSTOM_PROVIDER_CATALOG_ID ||
      row.kind.trim().toLowerCase() === CUSTOM_PROVIDER_CATALOG_ID,
  );
  return matches[0] ?? null;
}

export async function ensureConnection(
  kind: string,
  extras: {
    label?: string;
    catalogId?: string | null;
    baseUrl?: string | null;
  } = {},
): Promise<IntegrationConnection> {
  const existing =
    kind.trim().toLowerCase() === CUSTOM_PROVIDER_CATALOG_ID
      ? await findCustomConnection()
      : await findConnectionByKind(kind);
  if (existing) {
    const patch: {
      label?: string;
      catalog_id?: string | null;
      base_url?: string | null;
      kind?: string;
    } = {};
    if (extras.label && extras.label !== existing.label) patch.label = extras.label;
    if (extras.catalogId !== undefined && extras.catalogId !== existing.catalog_id) {
      patch.catalog_id = extras.catalogId;
    }
    if (extras.baseUrl !== undefined && extras.baseUrl !== (existing.base_url ?? null)) {
      patch.base_url = extras.baseUrl;
    }
    if (Object.keys(patch).length === 0) return existing;
    return daemonUnary<IntegrationConnection>(
      "integrations.by_connection_id.patch",
      { connection_id: existing.connection_id },
      patch,
    );
  }
  return daemonUnary<IntegrationConnection>("integrations.post", {}, {
    kind,
    label: extras.label,
    catalog_id: extras.catalogId,
    base_url: extras.baseUrl,
  });
}

export async function putIntegrationSecret(
  connectionId: string,
  slot: IntegrationSecretSlot,
  value: string,
): Promise<IntegrationSecretMutationResponse> {
  return daemonUnary<IntegrationSecretMutationResponse>(
    "integrations.by_connection_id.secrets.by_slot.put",
    { connection_id: connectionId, slot },
    { value },
  );
}

export async function deleteIntegrationSecret(
  connectionId: string,
  slot: IntegrationSecretSlot,
): Promise<IntegrationSecretMutationResponse> {
  return daemonUnary<IntegrationSecretMutationResponse>(
    "integrations.by_connection_id.secrets.by_slot.delete",
    { connection_id: connectionId, slot },
  );
}

export async function slotConfigured(
  kind: string,
  slot: IntegrationSecretSlot,
): Promise<boolean> {
  const connection = await findConnectionByKind(kind);
  return Boolean(connection?.secrets?.[slot]);
}
