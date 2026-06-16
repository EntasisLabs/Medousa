import {
  loadTuiDefaultsSummary,
  persistTuiVoicePrefs,
} from "$lib/config";
import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
import {
  allVoicePresets,
  DEFAULT_VOICE_ID,
  normalizeCustomVoicePresets,
  resolveVoicePreset,
  type VoicePreset,
} from "$lib/types/voicePresets";
import { isTauriMobilePlatform } from "$lib/platform";
import { isTauri } from "$lib/window";

const MOBILE_ACTIVE_VOICE_KEY = "medousa.activeVoiceId";

export class VoicePresetsStore {
  activeVoiceId = $state(DEFAULT_VOICE_ID);
  customPresets = $state<VoicePreset[]>([]);
  loaded = $state(false);
  saving = $state(false);

  allPresets = $derived(allVoicePresets(this.customPresets));

  activePreset = $derived(
    resolveVoicePreset(this.activeVoiceId, this.customPresets),
  );

  activeAppendix = $derived(this.activePreset.voiceAppendix.trim());

  async load(force = false) {
    if (!isTauri() || (this.loaded && !force)) return;
    try {
      if (workshopDefaults.loaded) {
        this.applyFromDraft(workshopDefaults.draft);
      } else {
        const summary = await loadTuiDefaultsSummary();
        this.activeVoiceId = summary.activeVoiceId?.trim() || DEFAULT_VOICE_ID;
        this.customPresets = normalizeCustomVoicePresets(summary.customVoicePresets);
      }
      if (isTauriMobilePlatform()) {
        const stored =
          typeof localStorage !== "undefined"
            ? localStorage.getItem(MOBILE_ACTIVE_VOICE_KEY)?.trim()
            : null;
        if (stored) this.activeVoiceId = stored;
      }
    } catch {
      // Keep built-in default when offline.
    }
    this.loaded = true;
  }

  applyFromDraft(draft: {
    activeVoiceId?: string | null;
    customVoicePresets?: VoicePreset[] | null;
  }) {
    this.activeVoiceId = draft.activeVoiceId?.trim() || DEFAULT_VOICE_ID;
    this.customPresets = normalizeCustomVoicePresets(draft.customVoicePresets);
  }

  syncFromWorkshopDraft() {
    if (!workshopDefaults.loaded) return;
    this.applyFromDraft(workshopDefaults.draft);
    this.loaded = true;
  }

  async setActiveVoiceId(nextId: string) {
    const preset = resolveVoicePreset(nextId, this.customPresets);
    if (preset.id === this.activeVoiceId) return;
    this.activeVoiceId = preset.id;
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      activeVoiceId: preset.id,
    };
    if (isTauriMobilePlatform() && typeof localStorage !== "undefined") {
      localStorage.setItem(MOBILE_ACTIVE_VOICE_KEY, preset.id);
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
}

export const voicePresets = new VoicePresetsStore();
