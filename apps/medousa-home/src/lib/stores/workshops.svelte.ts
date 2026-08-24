import { getDaemonUrl } from "$lib/daemon";
import {
  addLocalWorkshop,
  loadWorkshopRegistry,
  removeWorkshop,
  renameWorkshop,
  setActiveWorkshop,
  updateWorkshopBranding,
  updateWorkshopClientState,
} from "$lib/workshops";
import { requestWorkshopReconnect } from "$lib/runtime/workshopReconnectPort";
import { workshopSwitchPorts } from "$lib/runtime/workshopSwitchPorts";
import {
  activeWorkshop,
  defaultWorkshopRegistry,
  findWorkshop,
  MAX_WORKSHOPS,
  PERSONAL_WORKSHOP_ID,
  parseWorkshopRegistry,
  workshopMonogram,
  type WorkshopIcon,
  type WorkshopRegistry,
  type WorkshopServer,
} from "$lib/types/workshopRegistry";
import { isColorThemeId, type ColorThemeId } from "$lib/types/colorThemes";
import { isTauri } from "$lib/platform";
import { toast } from "$lib/runtime/toast.svelte";
import { completePairingFromQr, type PairCompleteFromQrResult } from "$lib/utils/pairingClient";
import { parsePairQrUrl } from "$lib/utils/pairingUrl";

export class WorkshopsStore {
  registry = $state<WorkshopRegistry>(defaultWorkshopRegistry());
  loading = $state(false);
  switching = $state(false);
  error = $state<string | null>(null);
  confirmSwitchId = $state<string | null>(null);
  /** After QR join — offer to switch to the new workshop. */
  pendingSwitchAfterPair = $state<string | null>(null);
  joinBusy = $state(false);
  joinError = $state<string | null>(null);

  activeWorkshop = $derived(activeWorkshop(this.registry));
  activeWorkshopId = $derived(this.registry.activeWorkshopId);
  /** Portal memberships only — peers are inbox-only and never appear here. */
  workshops = $derived(
    this.registry.workshops.filter(
      (workshop) => workshop.kind === "local" || workshop.kind === "portal" || workshop.kind === "paired",
    ),
  );
  hasMultipleWorkshops = $derived(this.workshops.length > 1);
  activeLabel = $derived(this.activeWorkshop?.label ?? "Personal");
  activeMonogram = $derived(workshopMonogram(this.activeLabel));
  activeBrandColor = $derived(this.activeWorkshop?.brandColor);
  activeColorThemeId = $derived(this.activeWorkshop?.clientState?.colorThemeId);
  atWorkshopLimit = $derived(this.registry.workshops.length >= MAX_WORKSHOPS);

  pendingSwitchAfterPairLabel = $derived.by(() => {
    const id = this.pendingSwitchAfterPair;
    if (!id) return null;
    return findWorkshop(this.registry, id)?.label ?? "New workshop";
  });

  async load() {
    if (!isTauri()) {
      this.registry = defaultWorkshopRegistry();
      return;
    }
    this.loading = true;
    this.error = null;
    try {
      this.registry = await loadWorkshopRegistry();
      workshopSwitchPorts().activateWorkshopScope(this.activeWorkshopId);
      const url = (await getDaemonUrl()).trim();
      if (url) workshopSwitchPorts().setDaemonUrl(url);
      this.applyThemeForActiveWorkshop();
      const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
      await shellTabs.switchWorkspaceScope(this.activeWorkshopId);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  needsSwitchConfirm(): boolean {
    const ports = workshopSwitchPorts();
    if (ports.vaultDirty()) return true;
    return ports.hasLiveInteractiveTurn();
  }

  requestSwitch(workshopId: string) {
    if (workshopId === this.activeWorkshopId) return;
    if (this.needsSwitchConfirm()) {
      this.confirmSwitchId = workshopId;
      return;
    }
    void this.selectWorkshop(workshopId);
  }

  cancelSwitchConfirm() {
    this.confirmSwitchId = null;
  }

  async confirmSwitch() {
    const id = this.confirmSwitchId;
    if (!id) return;
    this.confirmSwitchId = null;
    await this.selectWorkshop(id, { force: true });
  }

  dismissSwitchAfterPair() {
    this.pendingSwitchAfterPair = null;
  }

  async confirmSwitchAfterPair(
    onHealthChange?: (health: import("$lib/daemon").DaemonHealth | null) => void,
  ) {
    const id = this.pendingSwitchAfterPair;
    if (!id) return;
    this.pendingSwitchAfterPair = null;
    await this.selectWorkshop(id, { force: true, onHealthChange });
  }

  async joinFromPairLink(
    qrUrl: string,
    options?: { daemonUrl?: string; phoneName?: string },
  ): Promise<PairCompleteFromQrResult> {
    if (!isTauri()) {
      throw new Error("Joining workshops requires the Medousa app");
    }
    const trimmed = qrUrl.trim();
    const parsed = parsePairQrUrl(trimmed);
    if (!parsed) {
      throw new Error("Paste a valid medousa:// pairing link");
    }
    if (this.registry.workshops.length >= MAX_WORKSHOPS) {
      const existingId = `paired-${parsed.deviceId}`;
      if (!this.registry.workshops.some((workshop) => workshop.id === existingId)) {
        throw new Error(`Maximum of ${MAX_WORKSHOPS} workshops — remove one in Settings first.`);
      }
    }

    this.joinBusy = true;
    this.joinError = null;
    try {
      const daemonUrl = (options?.daemonUrl?.trim() || parsed.daemonUrl).replace(/\/+$/, "");
      const result = await completePairingFromQr({
        qrUrl: trimmed,
        daemonUrl,
        phoneName: options?.phoneName,
        role: "portal",
      });
      await this.onPairComplete(result);
      return result;
    } catch (err) {
      this.joinError = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.joinBusy = false;
    }
  }

  async onPairComplete(result: PairCompleteFromQrResult) {
    await this.load();
    if (result.workshopId && result.workshopId !== this.activeWorkshopId) {
      this.pendingSwitchAfterPair = result.workshopId;
    }
  }

  async selectWorkshop(
    workshopId: string,
    options?: {
      force?: boolean;
      onHealthChange?: (health: import("$lib/daemon").DaemonHealth | null) => void;
    },
  ) {
    if (!isTauri()) return;
    if (workshopId === this.activeWorkshopId) return;
    if (this.switching) return;
    if (!options?.force && this.needsSwitchConfirm()) {
      this.confirmSwitchId = workshopId;
      return;
    }

    this.switching = true;
    this.error = null;
    const previousWorkshopId = this.activeWorkshopId;
    let selectionCommitted = false;
    try {
      const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
      const ports = workshopSwitchPorts();
      const flushed = await ports.flushVaultBeforeLeave();
      if (!flushed) return;
      shellTabs.checkpoint();
      await ports.prepareForWorkshopSwitch();
      this.registry = await setActiveWorkshop(workshopId);
      selectionCommitted = true;
      ports.activateWorkshopScope(this.activeWorkshopId);
      await shellTabs.switchWorkspaceScope(this.activeWorkshopId);
      const url = (await getDaemonUrl()).trim();
      if (url) ports.setDaemonUrl(url);
      await requestWorkshopReconnect((health) => {
        options?.onHealthChange?.(health);
      });
      this.applyThemeForActiveWorkshop();
      toast.show(`Switched to ${this.activeLabel}`);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      const ports = workshopSwitchPorts();
      const recoveryWorkshopId = selectionCommitted
        ? this.activeWorkshopId
        : previousWorkshopId;
      ports.activateWorkshopScope(recoveryWorkshopId);
      const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
      await shellTabs.switchWorkspaceScope(recoveryWorkshopId).catch(() => undefined);
      await requestWorkshopReconnect(options?.onHealthChange).catch(() => null);
      throw err;
    } finally {
      this.switching = false;
    }
  }

  async saveActiveSession(sessionId: string) {
    if (!isTauri()) return;
    const trimmed = sessionId.trim();
    if (!trimmed || trimmed === this.activeWorkshop?.clientState?.lastSessionId) return;
    try {
      this.registry = await updateWorkshopClientState(this.activeWorkshopId, {
        lastSessionId: trimmed,
      });
    } catch {
      // Best-effort — session still works locally.
    }
  }

  applyThemeForActiveWorkshop() {
    const themeId = this.activeWorkshop?.clientState?.colorThemeId;
    const ports = workshopSwitchPorts();
    if (isColorThemeId(themeId)) {
      ports.setColorTheme(themeId, { persistWorkshop: false });
      return;
    }
    ports.applyTheme();
  }

  async saveColorTheme(themeId: ColorThemeId) {
    if (!isTauri()) return;
    try {
      this.registry = await updateWorkshopClientState(this.activeWorkshopId, {
        colorThemeId: themeId,
      });
    } catch {
      // Theme still applied locally.
    }
  }

  async updateBranding(
    workshopId: string,
    patch: {
      icon?: WorkshopIcon | null;
      brandColor?: string | null;
      tagline?: string | null;
    },
  ) {
    this.registry = await updateWorkshopBranding(workshopId, patch);
  }

  async restoreLastSession() {
    const lastId = this.activeWorkshop?.clientState?.lastSessionId?.trim();
    if (!lastId) return;
    const ports = workshopSwitchPorts();
    if (ports.chatSessionId() === lastId) return;
    if (!ports.chatHasSession(lastId)) return;
    await ports.switchChatSession(lastId);
  }

  async addLocalEngine(label: string, dataDir: string) {
    this.error = null;
    try {
      this.registry = await addLocalWorkshop(label, dataDir);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }

  async renameWorkshop(workshopId: string, label: string) {
    this.error = null;
    try {
      this.registry = await renameWorkshop(workshopId, label);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }

  async removeWorkshop(
    workshopId: string,
    options?: { onHealthChange?: (health: import("$lib/daemon").DaemonHealth | null) => void },
  ) {
    if (workshopId === PERSONAL_WORKSHOP_ID) return;
    const wasActive = workshopId === this.activeWorkshopId;
    let shellTabs: {
      checkpoint: () => void;
      switchWorkspaceScope: (scopeId: string) => Promise<void>;
    } | null = null;
    this.error = null;
    try {
      if (wasActive) {
        const ports = workshopSwitchPorts();
        const flushed = await ports.flushVaultBeforeLeave();
        if (!flushed) return;
        ({ shellTabs } = await import("$lib/stores/shellTabs.svelte"));
        shellTabs.checkpoint();
        await ports.prepareForWorkshopSwitch();
      }
      this.registry = await removeWorkshop(workshopId);
      if (wasActive) {
        workshopSwitchPorts().activateWorkshopScope(this.activeWorkshopId);
        await shellTabs?.switchWorkspaceScope(this.activeWorkshopId);
        const url = (await getDaemonUrl()).trim();
        if (url) workshopSwitchPorts().setDaemonUrl(url);
        await requestWorkshopReconnect((health) => {
          options?.onHealthChange?.(health);
        });
      }
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      if (wasActive) {
        workshopSwitchPorts().activateWorkshopScope(this.activeWorkshopId);
        await shellTabs?.switchWorkspaceScope(this.activeWorkshopId).catch(() => undefined);
        await requestWorkshopReconnect(options?.onHealthChange).catch(() => null);
      }
    }
  }

  applyRegistry(raw: unknown) {
    const parsed = parseWorkshopRegistry(raw);
    if (parsed) this.registry = parsed;
  }
}

export const workshops = new WorkshopsStore();
