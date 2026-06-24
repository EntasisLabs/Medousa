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
import { reconnectWorkshop } from "$lib/workshopConnection";
import { chat } from "$lib/stores/chat.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { settings } from "$lib/stores/settings.svelte";
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
import { toast } from "$lib/stores/toast.svelte";
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
  workshops = $derived(this.registry.workshops);
  hasMultipleWorkshops = $derived(this.registry.workshops.length > 1);
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
      const url = (await getDaemonUrl()).trim();
      if (url) settings.daemonUrl = url;
      this.applyThemeForActiveWorkshop();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  needsSwitchConfirm(): boolean {
    if (vault.dirty) return true;
    return chat.hasLiveInteractiveTurn();
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
    if (!options?.force && this.needsSwitchConfirm()) {
      this.confirmSwitchId = workshopId;
      return;
    }

    this.switching = true;
    this.error = null;
    try {
      this.registry = await setActiveWorkshop(workshopId);
      const url = (await getDaemonUrl()).trim();
      if (url) settings.daemonUrl = url;
      await reconnectWorkshop((health) => {
        options?.onHealthChange?.(health);
      });
      this.applyThemeForActiveWorkshop();
      await this.restoreLastSession();
      toast.show(`Connected to ${this.activeLabel}`);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
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
    if (isColorThemeId(themeId)) {
      settings.setColorTheme(themeId, { persistWorkshop: false });
      return;
    }
    settings.applyTheme();
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
    if (chat.sessionId === lastId) return;
    const exists = chat.sessions.some((session) => session.session_id === lastId);
    if (!exists) return;
    await chat.switchSession(lastId);
  }

  async addLocalEngine(label: string, dataDir: string) {
    this.registry = await addLocalWorkshop(label, dataDir);
  }

  async renameWorkshop(workshopId: string, label: string) {
    this.registry = await renameWorkshop(workshopId, label);
  }

  async removeWorkshop(
    workshopId: string,
    options?: { onHealthChange?: (health: import("$lib/daemon").DaemonHealth | null) => void },
  ) {
    if (workshopId === PERSONAL_WORKSHOP_ID) return;
    const wasActive = workshopId === this.activeWorkshopId;
    this.registry = await removeWorkshop(workshopId);
    if (wasActive) {
      const url = (await getDaemonUrl()).trim();
      if (url) settings.daemonUrl = url;
      await reconnectWorkshop((health) => {
        options?.onHealthChange?.(health);
      });
    }
  }

  applyRegistry(raw: unknown) {
    const parsed = parseWorkshopRegistry(raw);
    if (parsed) this.registry = parsed;
  }
}

export const workshops = new WorkshopsStore();
