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
}

export interface PairingStatusResponse {
  pairedDevices: PairedDeviceSummary[];
  qrActive: boolean;
  deviceId: string;
  peerName: string;
  protocolVersion: string;
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

export async function fetchPairingQr(): Promise<PairingQrResponse> {
  if (!isTauri()) {
    throw new Error("Pairing requires the Medousa Home desktop app");
  }
  return invoke<PairingQrResponse>("pairing_fetch_qr");
}

export async function fetchPairingQrImage(): Promise<PairingQrImage> {
  if (!isTauri()) {
    throw new Error("Pairing requires the Medousa Home desktop app");
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
    };
  }
  return invoke<PairingStatusResponse>("pairing_fetch_status");
}

export async function revokePairingDevice(pairingId: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("pairing_revoke", { pairingId });
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
