import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export interface PairingQrResponse {
  url: string;
  expiresAt: string;
  shortCode: string;
}

export interface PairedDeviceSummary {
  pairingId: string;
  phoneId: string;
  phoneName: string;
  pairedAt: string;
  lastSeen: string;
  sessionExpiresAt?: string | null;
  trustExpiresAt?: string | null;
  idleTimeoutSeconds?: number | null;
  trustActive: boolean;
  role?: string | null;
  profileId?: string | null;
}

export interface PairingStatusResponse {
  pairedDevices: PairedDeviceSummary[];
  qrActive: boolean;
  deviceId: string;
  peerName: string;
  protocolVersion: string;
  daemonPublicKey: string;
  irohAvailable: boolean;
  qrProtocolVersion: string;
}

export type PeerExecutionPolicyPreset =
  | "connected_only"
  | "assistant_work"
  | "sandboxed_work"
  | "approved_projects"
  | "custom";

export type PeerNetworkPolicy = "deny" | "web_only" | "unrestricted";

export interface PeerExecutionPolicy {
  schemaVersion: number;
  peerDeviceId: string;
  peerPairingId: string;
  preset: PeerExecutionPolicyPreset;
  enabled: boolean;
  assistantWork: boolean;
  sandboxExecution: boolean;
  hostShell: boolean;
  coderWork: boolean;
  workEnvironmentMaterialization: boolean;
  allowedProjectIds?: string[];
  allowedRootRefs?: string[];
  allowedToolDomains?: string[];
  allowedMcpServerIds?: string[];
  allowedSecretRefs?: string[];
  networkPolicy: PeerNetworkPolicy;
  allowAgentTargeting: boolean;
  expiresAt?: string | null;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface PeerExecutionPolicyEntry {
  peerDeviceId: string;
  pairingId: string;
  displayName: string;
  role: string;
  pairedAt: string;
  lastSeen: string;
  meshEnabled: boolean;
  meshGrants: string[];
  legacyTaskRequestGranted: boolean;
  execution: {
    policy: PeerExecutionPolicy;
    source: "stored" | "legacy_task_request" | "default_deny";
  };
}

export interface PeerExecutionPolicyUpdate {
  preset: PeerExecutionPolicyPreset;
  enabled?: boolean;
  assistantWork?: boolean;
  sandboxExecution?: boolean;
  hostShell?: boolean;
  coderWork?: boolean;
  workEnvironmentMaterialization?: boolean;
  allowedProjectIds?: string[];
  allowedRootRefs?: string[];
  allowedToolDomains?: string[];
  allowedMcpServerIds?: string[];
  allowedSecretRefs?: string[];
  networkPolicy?: PeerNetworkPolicy;
  allowAgentTargeting?: boolean;
  expiresAt?: string | null;
}

export interface PeerExecutionPolicyUpdateResponse {
  peer: PeerExecutionPolicyEntry;
  cancelledWorkCount: number;
}

export interface IrohTicketResponse {
  ticket: string;
  endpointId: string;
  available: boolean;
}

export interface PairingQrImage {
  dataUrl: string;
  url: string;
  expiresAt: string;
  shortCode: string;
}

export interface BonjourStatus {
  pairingAvailable: boolean;
  likelyAdvertising: boolean;
  serviceType: string;
  deviceId?: string | null;
  peerName?: string | null;
  message: string;
}

export async function rotatePairingInvite(options?: {
  profileId?: string;
}): Promise<PairingQrResponse> {
  if (!isTauri()) {
    throw new Error("Pairing requires the Medousa desktop app");
  }
  return invoke<PairingQrResponse>("pairing_rotate_invite", {
    profileId: options?.profileId?.trim() || undefined,
  });
}

export async function fetchPairingQr(options?: {
  /** Embed Iroh ticket (large). Default is compact camera-friendly invite. */
  full?: boolean;
}): Promise<PairingQrResponse> {
  if (!isTauri()) {
    throw new Error("Pairing requires the Medousa desktop app");
  }
  return invoke<PairingQrResponse>("pairing_fetch_qr", {
    full: options?.full ?? false,
  });
}

export async function fetchPairingQrImage(): Promise<PairingQrImage> {
  if (!isTauri()) {
    throw new Error("Pairing requires the Medousa desktop app");
  }
  return invoke<PairingQrImage>("pairing_fetch_qr_image");
}

export async function fetchPairingStatus(): Promise<PairingStatusResponse> {
  if (!isTauri()) {
    return {
      pairedDevices: [],
      qrActive: false,
      deviceId: "",
      peerName: "",
      protocolVersion: "1.0.0",
      daemonPublicKey: "",
      irohAvailable: false,
      qrProtocolVersion: "1.0",
    };
  }
  return invoke<PairingStatusResponse>("pairing_fetch_status");
}

export async function revokePairingDevice(pairingId: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("pairing_revoke", { pairingId });
}

export async function updatePairingPolicy(
  pairingId: string,
  policy: {
    trustExpiresAt: string | null;
    idleTimeoutSeconds: number | null;
  },
): Promise<PairedDeviceSummary> {
  if (!isTauri()) {
    throw new Error("Pairing trust settings require the Medousa app");
  }
  return invoke<PairedDeviceSummary>("pairing_update_policy", {
    pairingId,
    trustExpiresAt: policy.trustExpiresAt,
    idleTimeoutSeconds: policy.idleTimeoutSeconds,
  });
}

export async function fetchPeerExecutionPolicies(): Promise<PeerExecutionPolicyEntry[]> {
  if (!isTauri()) return [];
  const response = await invoke<{ peers: PeerExecutionPolicyEntry[] }>(
    "pairing_fetch_execution_policies",
  );
  return response.peers ?? [];
}

export async function updatePeerExecutionPolicy(
  deviceId: string,
  policy: PeerExecutionPolicyUpdate,
): Promise<PeerExecutionPolicyUpdateResponse> {
  if (!isTauri()) {
    throw new Error("Peer permissions require the Medousa app");
  }
  return invoke<PeerExecutionPolicyUpdateResponse>("pairing_update_execution_policy", {
    deviceId,
    policy,
  });
}

export async function fetchBonjourStatus(): Promise<BonjourStatus> {
  if (!isTauri()) {
    return {
      pairingAvailable: false,
      likelyAdvertising: false,
      serviceType: "_medousa._tcp.local.",
      message: "Bonjour status requires the desktop app",
    };
  }
  return invoke<BonjourStatus>("bonjour_status");
}

/** Poll until /qr/image succeeds — status alone can pass before QR generation is ready. */
export async function waitForPairingQr(timeoutSeconds = 45): Promise<PairingQrImage> {
  if (!isTauri()) {
    throw new Error("Pairing requires the Medousa desktop app");
  }
  return invoke<PairingQrImage>("pairing_wait_ready", { timeoutSeconds });
}

export function formatShortCode(raw: string): string {
  const cleaned = raw.replace(/[^A-Za-z0-9]/g, "").toUpperCase();
  if (cleaned.length <= 3) return cleaned;
  if (cleaned.length <= 6) {
    return `${cleaned.slice(0, 3)}-${cleaned.slice(3)}`;
  }
  return `${cleaned.slice(0, 3)}-${cleaned.slice(3, 6)}-${cleaned.slice(6, 9)}`;
}

export function secondsUntil(iso: string): number {
  const target = Date.parse(iso);
  if (Number.isNaN(target)) return 0;
  return Math.max(0, Math.floor((target - Date.now()) / 1000));
}

export function formatCountdown(totalSeconds: number): string {
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
