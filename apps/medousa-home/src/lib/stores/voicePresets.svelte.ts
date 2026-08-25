import {
  loadTuiDefaultsSummary,
  persistTuiVoicePrefs,
} from "$lib/config";
import {
  allVoicePresets,
  DEFAULT_VOICE_ID,
  normalizeCustomVoicePresets,
  resolveVoicePreset,
  type VoicePreset,
} from "$lib/types/voicePresets";
import { isTauriMobilePlatform } from "$lib/platform";
import { isTauri } from "$lib/window";
import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";

const MOBILE_ACTIVE_VOICE_KEY = "medousa.activeVoiceId";

function mobileActiveVoiceKey(): string {
  return workshopScopedStorageKey(MOBILE_ACTIVE_VOICE_KEY);
}

export class VoicePresetsStore {
  activeVoiceId = $state(DEFAULT_VOICE_ID);
  customPresets = $state<VoicePreset[]>([]);
  loaded = $state(false);
  saving = $state(false);
  private loadEpoch = 0;

  allPresets = $derived(allVoicePresets(this.customPresets));

  activePreset = $derived(
    resolveVoicePreset(this.activeVoiceId, this.customPresets),
  );

  activeAppendix = $derived(this.activePreset.voiceAppendix.trim());

  async load(force = false) {
    if (!isTauri() || (this.loaded && !force)) return;
    const loadEpoch = ++this.loadEpoch;
    try {
      const summary = await loadTuiDefaultsSummary();
      if (loadEpoch !== this.loadEpoch) return;
      this.applyFromDraft({
        activeVoiceId: summary.activeVoiceId,
        customVoicePresets: summary.customVoicePresets,
      });
      if (isTauriMobilePlatform()) {
        const stored =
          typeof localStorage !== "undefined"
            ? localStorage.getItem(mobileActiveVoiceKey())?.trim()
            : null;
        if (stored) this.activeVoiceId = stored;
      }
    } catch {
      if (loadEpoch !== this.loadEpoch) return;
      // Keep built-in default when offline.
    }
    if (loadEpoch === this.loadEpoch) {
      this.loaded = true;
    }
  }

  applyFromDraft(draft: {
    activeVoiceId?: string | null;
    customVoicePresets?: VoicePreset[] | null;
  }) {
    this.activeVoiceId = draft.activeVoiceId?.trim() || DEFAULT_VOICE_ID;
    this.customPresets = normalizeCustomVoicePresets(draft.customVoicePresets);
  }

  async setActiveVoiceId(nextId: string) {
    const preset = resolveVoicePreset(nextId, this.customPresets);
    if (preset.id === this.activeVoiceId) return;
    this.activeVoiceId = preset.id;
    if (isTauriMobilePlatform() && typeof localStorage !== "undefined") {
      localStorage.setItem(mobileActiveVoiceKey(), preset.id);
    }
    await this.persistActiveVoice();
  }

  turnVoiceFields(): { voicePresetId: string; voiceAppendix?: string } {
    const preset = this.activePreset;
    const appendix = preset.voiceAppendix.trim();
    return appendix
      ? { voicePresetId: preset.id, voiceAppendix: appendix }
      : { voicePresetId: preset.id };
  }

  private async persistActiveVoice() {
    if (!isTauri() || isTauriMobilePlatform()) return;
    this.saving = true;
    try {
      await persistTuiVoicePrefs({
        activeVoiceId: this.activeVoiceId,
        customVoicePresets: this.customPresets,
      });
    } finally {
      this.saving = false;
    }
  }

  resetForWorkshopSwitch() {
    this.loadEpoch += 1;
    this.activeVoiceId = DEFAULT_VOICE_ID;
    this.customPresets = [];
    this.loaded = false;
    this.saving = false;
  }
}

export const voicePresets = new VoicePresetsStore();
