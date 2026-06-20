import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export interface PairCompleteFromQrRequest {
  qrUrl: string;
  daemonUrl: string;
  phoneName?: string;
}

export interface PairCompleteFromQrResult {
  pairingId: string;
  phoneId: string;
  workshopDeviceId: string;
  workshopId: string;
  workshopPeerName: string;
  daemonUrl: string;
}

export interface PairingCredentialsSummary {
  pairingId: string;
  phoneId: string;
  workshopDeviceId: string;
  daemonUrl: string;
  pairedAt: string;
  hasSessionToken: boolean;
  irohAvailable: boolean;
}

/** Run the Ed25519 init/verify ceremony after parsing a medousa:// pairing link. */
export async function completePairingFromQr(
  request: PairCompleteFromQrRequest,
): Promise<PairCompleteFromQrResult> {
  if (!isTauri()) {
    throw new Error("Pairing requires the Medousa mobile app");
  }
  return invoke<PairCompleteFromQrResult>("pairing_complete_from_qr", { request });
}

export async function loadPairingCredentials(): Promise<PairingCredentialsSummary | null> {
  if (!isTauri()) return null;
  return invoke<PairingCredentialsSummary | null>("pairing_load_credentials");
}

/** Tell the workshop we're alive — routes over LAN or Iroh when off-network. */
export async function sendPairingHeartbeat(): Promise<void> {
  if (!isTauri()) return;
  await invoke("pairing_send_heartbeat");
}
