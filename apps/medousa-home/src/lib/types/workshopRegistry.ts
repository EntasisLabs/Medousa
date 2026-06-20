/**
 * Workshop registry — client-side multi-engine connection list.
 * Design lock for M1; persisted by Tauri as workshops.json (ADR-003).
 */

export const WORKSHOP_REGISTRY_VERSION = 1 as const;

/** Reserved id for the default local engine — never removed. */
export const PERSONAL_WORKSHOP_ID = "personal";

export const MAX_WORKSHOPS = 10;

export const DEFAULT_LOCAL_DAEMON_URL = "http://127.0.0.1:7419";

export type WorkshopKind = "local" | "paired";

export type WorkshopIcon = "home" | "building" | "team";

/** Pairing metadata; secrets live in pairing.json + keychain. */
export interface WorkshopPairingRef {
  pairingId: string;
  phoneId: string;
  workshopDeviceId: string;
  pairedAt: string;
  /** Relative to medousa data dir, e.g. `workshops/{id}/pairing.json` */
  credentialsRelPath?: string;
  hasIrohTicket?: boolean;
  workshopPeerName?: string;
}

/** Per-workshop client hints (not synced across devices in v1). */
export interface WorkshopClientState {
  lastSessionId?: string;
  colorThemeId?: string;
}

export interface WorkshopServer {
  id: string;
  label: string;
  kind: WorkshopKind;
  /** Canonical HTTP base (no trailing slash). */
  url: string;
  icon?: WorkshopIcon;
  createdAt: string;
  updatedAt: string;
  lastConnectedAt?: string;
  pairing?: WorkshopPairingRef;
  clientState?: WorkshopClientState;
}

export interface WorkshopRegistry {
  version: typeof WORKSHOP_REGISTRY_VERSION;
  activeWorkshopId: string;
  workshops: WorkshopServer[];
}

export function pairingCredentialsRelPath(workshopId: string): string {
  return `workshops/${workshopId}/pairing.json`;
}

export function pairedWorkshopId(workshopDeviceId: string): string {
  return `paired-${workshopDeviceId}`;
}

export function defaultPersonalWorkshop(now = new Date()): WorkshopServer {
  const iso = now.toISOString();
  return {
    id: PERSONAL_WORKSHOP_ID,
    label: "Personal",
    kind: "local",
    url: DEFAULT_LOCAL_DAEMON_URL,
    icon: "home",
    createdAt: iso,
    updatedAt: iso,
  };
}

export function defaultWorkshopRegistry(now = new Date()): WorkshopRegistry {
  const personal = defaultPersonalWorkshop(now);
  return {
    version: WORKSHOP_REGISTRY_VERSION,
    activeWorkshopId: PERSONAL_WORKSHOP_ID,
    workshops: [personal],
  };
}

export function normalizeWorkshopUrl(raw: string): string {
  return raw.trim().replace(/\/+$/, "");
}

export function findWorkshop(
  registry: WorkshopRegistry,
  id: string,
): WorkshopServer | undefined {
  return registry.workshops.find((workshop) => workshop.id === id);
}

export function activeWorkshop(registry: WorkshopRegistry): WorkshopServer | undefined {
  return findWorkshop(registry, registry.activeWorkshopId);
}

function isWorkshopKind(value: unknown): value is WorkshopKind {
  return value === "local" || value === "paired";
}

function isWorkshopIcon(value: unknown): value is WorkshopIcon {
  return value === "home" || value === "building" || value === "team";
}

function parsePairingRef(raw: unknown): WorkshopPairingRef | undefined {
  if (!raw || typeof raw !== "object") return undefined;
  const record = raw as Record<string, unknown>;
  if (
    typeof record.pairingId !== "string" ||
    typeof record.phoneId !== "string" ||
    typeof record.workshopDeviceId !== "string" ||
    typeof record.pairedAt !== "string"
  ) {
    return undefined;
  }
  return {
    pairingId: record.pairingId,
    phoneId: record.phoneId,
    workshopDeviceId: record.workshopDeviceId,
    pairedAt: record.pairedAt,
    credentialsRelPath:
      typeof record.credentialsRelPath === "string"
        ? record.credentialsRelPath
        : undefined,
    hasIrohTicket:
      typeof record.hasIrohTicket === "boolean" ? record.hasIrohTicket : undefined,
    workshopPeerName:
      typeof record.workshopPeerName === "string"
        ? record.workshopPeerName
        : undefined,
  };
}

function parseWorkshop(raw: unknown): WorkshopServer | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  if (
    typeof record.id !== "string" ||
    typeof record.label !== "string" ||
    !isWorkshopKind(record.kind) ||
    typeof record.url !== "string" ||
    typeof record.createdAt !== "string" ||
    typeof record.updatedAt !== "string"
  ) {
    return null;
  }

  const pairing = parsePairingRef(record.pairing);
  if (record.kind === "paired" && !pairing) return null;

  return {
    id: record.id,
    label: record.label,
    kind: record.kind,
    url: normalizeWorkshopUrl(record.url),
    icon: isWorkshopIcon(record.icon) ? record.icon : undefined,
    createdAt: record.createdAt,
    updatedAt: record.updatedAt,
    lastConnectedAt:
      typeof record.lastConnectedAt === "string" ? record.lastConnectedAt : undefined,
    pairing,
    clientState:
      record.clientState && typeof record.clientState === "object"
        ? (record.clientState as WorkshopClientState)
        : undefined,
  };
}

/** Parse workshops.json from disk; returns null if invalid. */
export function parseWorkshopRegistry(raw: unknown): WorkshopRegistry | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  if (record.version !== WORKSHOP_REGISTRY_VERSION) return null;
  if (typeof record.activeWorkshopId !== "string") return null;
  if (!Array.isArray(record.workshops) || record.workshops.length === 0) return null;

  const workshops: WorkshopServer[] = [];
  for (const entry of record.workshops) {
    const parsed = parseWorkshop(entry);
    if (!parsed) return null;
    workshops.push(parsed);
  }

  if (!workshops.some((workshop) => workshop.id === record.activeWorkshopId)) {
    return null;
  }

  return {
    version: WORKSHOP_REGISTRY_VERSION,
    activeWorkshopId: record.activeWorkshopId,
    workshops,
  };
}

export function workshopMonogram(label: string): string {
  const parts = label.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0]!.slice(0, 1).toUpperCase();
  return `${parts[0]!.slice(0, 1)}${parts[1]!.slice(0, 1)}`.toUpperCase();
}
