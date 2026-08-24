/** Ports so workshops does not import chat/vault/settings stores. */

import type { ColorThemeId } from "$lib/types/colorThemes";

export type WorkshopSwitchPorts = {
  vaultDirty: () => boolean;
  flushVaultBeforeLeave: () => Promise<boolean>;
  prepareForWorkshopSwitch: () => Promise<void>;
  activateWorkshopScope: (workshopId: string) => void;
  hasLiveInteractiveTurn: () => boolean;
  chatSessionId: () => string;
  chatHasSession: (sessionId: string) => boolean;
  switchChatSession: (sessionId: string) => Promise<void>;
  setDaemonUrl: (url: string) => void;
  setColorTheme: (
    themeId: ColorThemeId,
    options?: { persistWorkshop?: boolean },
  ) => void;
  applyTheme: () => void;
};

const unbound: WorkshopSwitchPorts = {
  vaultDirty: () => false,
  flushVaultBeforeLeave: async () => true,
  prepareForWorkshopSwitch: async () => {},
  activateWorkshopScope: () => {},
  hasLiveInteractiveTurn: () => false,
  chatSessionId: () => "",
  chatHasSession: () => false,
  switchChatSession: async () => {},
  setDaemonUrl: () => {},
  setColorTheme: () => {},
  applyTheme: () => {},
};

let ports: WorkshopSwitchPorts | null = null;

export function setWorkshopSwitchPorts(next: WorkshopSwitchPorts | null): void {
  ports = next;
}

export function workshopSwitchPorts(): WorkshopSwitchPorts {
  return ports ?? unbound;
}
