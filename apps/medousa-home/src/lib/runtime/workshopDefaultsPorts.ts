/** Ports so workshop defaults do not import runtime/voice, and siblings do not import defaults. */

import type { TuiDefaults } from "$lib/types/workshopDefaults";

export type WorkshopDefaultsSyncPort = {
  applyVoiceDraft: (draft: TuiDefaults) => void;
  applyRuntimeFromDefaults: (payload: TuiDefaults) => Promise<void>;
};

export type WorkshopDefaultsQueryPort = {
  loaded: () => boolean;
  dirty: () => boolean;
  workCardHideAfterHours: () => number | null | undefined;
  workCardWipeAfterDays: () => number | null | undefined;
  vaultGitEnabled: () => boolean | null | undefined;
  setVaultGitEnabled: (enabled: boolean) => void;
  save: () => Promise<void>;
  resetForReconnect: () => void;
  load: (force?: boolean) => Promise<void>;
};

const unboundSync: WorkshopDefaultsSyncPort = {
  applyVoiceDraft: () => {},
  applyRuntimeFromDefaults: async () => {},
};

const unboundQuery: WorkshopDefaultsQueryPort = {
  loaded: () => false,
  dirty: () => false,
  workCardHideAfterHours: () => null,
  workCardWipeAfterDays: () => null,
  vaultGitEnabled: () => null,
  setVaultGitEnabled: () => {},
  save: async () => {},
  resetForReconnect: () => {},
  load: async () => {},
};

let sync: WorkshopDefaultsSyncPort | null = null;
let query: WorkshopDefaultsQueryPort | null = null;

export function setWorkshopDefaultsSyncPort(next: WorkshopDefaultsSyncPort | null): void {
  sync = next;
}

export function setWorkshopDefaultsQueryPort(next: WorkshopDefaultsQueryPort | null): void {
  query = next;
}

export function workshopDefaultsSyncPort(): WorkshopDefaultsSyncPort {
  return sync ?? unboundSync;
}

export function workshopDefaultsQueryPort(): WorkshopDefaultsQueryPort {
  return query ?? unboundQuery;
}
