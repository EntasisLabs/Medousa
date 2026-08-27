import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export interface MeshPeerEndpoints {
  lanBaseUrl?: string | null;
  irohTicket?: string | null;
  irohEndpointId?: string | null;
}

export interface MeshIntroRecord {
  id: string;
  requesterDeviceId: string;
  requesterDisplayName: string;
  targetDeviceId: string;
  targetDisplayName: string;
  status: "pending" | "accepted" | "declined" | "expired" | string;
  note?: string | null;
  createdAt: string;
  expiresAt: string;
  acceptedAt?: string | null;
  requesterEndpoints?: MeshPeerEndpoints;
  targetEndpoints?: MeshPeerEndpoints;
  /** `"requester"` | `"target"` for the authenticated caller. */
  youAre?: "requester" | "target" | string | null;
}

export interface MeshIntroCandidate {
  deviceId: string;
  displayName: string;
  role: string;
  lastSeen: string;
}

export interface IntroWorkshopSummary {
  workshopId: string;
  label: string;
  kind: string;
  hasSessionToken: boolean;
}

export interface MeshPeerGrantRow {
  deviceId: string;
  displayName: string;
  role: string;
  meshGrants: string[];
  rendezvous: boolean;
  taskRequest: boolean;
  lastSeen: string;
}

function requireTauri(): void {
  if (!isTauri()) {
    throw new Error("Workshop introductions require the Medousa desktop app");
  }
}

export async function listIntroWorkshops(): Promise<IntroWorkshopSummary[]> {
  requireTauri();
  return invoke<IntroWorkshopSummary[]>("list_intro_workshops");
}

export async function meshListIntros(
  workshopId: string,
  status?: string,
): Promise<MeshIntroRecord[]> {
  requireTauri();
  return invoke<MeshIntroRecord[]>("mesh_list_intros", {
    workshopId,
    status: status ?? null,
  });
}

export async function meshListIntroCandidates(
  workshopId: string,
): Promise<MeshIntroCandidate[]> {
  requireTauri();
  return invoke<MeshIntroCandidate[]>("mesh_list_intro_candidates", { workshopId });
}

export async function meshRequestIntro(
  workshopId: string,
  targetDeviceId: string,
  note?: string | null,
): Promise<MeshIntroRecord> {
  requireTauri();
  return invoke<MeshIntroRecord>("mesh_request_intro", {
    workshopId,
    targetDeviceId,
    note: note ?? null,
  });
}

export async function meshAcceptIntro(
  workshopId: string,
  introId: string,
): Promise<MeshIntroRecord> {
  requireTauri();
  return invoke<MeshIntroRecord>("mesh_accept_intro", { workshopId, introId });
}

export async function meshDeclineIntro(
  workshopId: string,
  introId: string,
): Promise<MeshIntroRecord> {
  requireTauri();
  return invoke<MeshIntroRecord>("mesh_decline_intro", { workshopId, introId });
}

export async function meshListLocalPeers(): Promise<MeshPeerGrantRow[]> {
  requireTauri();
  return invoke<MeshPeerGrantRow[]>("mesh_list_local_peers");
}

export async function meshSetPeerRendezvous(
  deviceId: string,
  enabled: boolean,
): Promise<MeshPeerGrantRow> {
  requireTauri();
  return invoke<MeshPeerGrantRow>("mesh_set_peer_rendezvous", { deviceId, enabled });
}

export async function meshSetPeerTaskRequest(
  deviceId: string,
  enabled: boolean,
): Promise<MeshPeerGrantRow> {
  requireTauri();
  return invoke<MeshPeerGrantRow>("mesh_set_peer_task_request", { deviceId, enabled });
}

export function oppositeEndpoints(
  intro: MeshIntroRecord,
  localDeviceIdHint?: string | null,
): MeshPeerEndpoints | null {
  const hint = localDeviceIdHint?.trim() ?? "";
  if (hint && intro.requesterDeviceId.startsWith(hint.slice(0, 8))) {
    return intro.targetEndpoints ?? null;
  }
  if (hint && intro.targetDeviceId.startsWith(hint.slice(0, 8))) {
    return intro.requesterEndpoints ?? null;
  }
  // Prefer the non-empty side when we don't know local identity.
  const target = intro.targetEndpoints;
  const requester = intro.requesterEndpoints;
  if (target?.lanBaseUrl || target?.irohTicket) return target;
  if (requester?.lanBaseUrl || requester?.irohTicket) return requester;
  return null;
}
