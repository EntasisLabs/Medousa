import { getSharedMode, setSharedMode, type SharedModeStatus } from "$lib/daemon";
import { isTauri } from "$lib/window";
import { userProfiles } from "$lib/stores/userProfiles.svelte";
import {
  friendlySettingsError,
  isMissingCapabilityError,
} from "$lib/utils/normieErrors";

export class SharedModeStore {
  mode = $state<"personal" | "shared">("personal");
  rootProfileId = $state("user:root");
  generalProfileId = $state("user:general");
  enabledAt = $state<string | null>(null);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);
  /** Older engines without /v1/shared-mode. */
  unsupported = $state(false);

  isShared = $derived(this.mode === "shared");

  applyStatus(status: SharedModeStatus) {
    this.mode = status.mode === "shared" ? "shared" : "personal";
    this.rootProfileId = status.root_profile_id;
    this.generalProfileId = status.general_profile_id;
    this.enabledAt = status.enabled_at ?? null;
    this.unsupported = false;
  }

  async load() {
    if (!isTauri()) return;
    this.loading = true;
    this.error = null;
    try {
      const status = await getSharedMode();
      this.applyStatus(status);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      // Older engines may not expose shared mode yet — stay personal, stay quiet.
      if (isMissingCapabilityError(message)) {
        this.mode = "personal";
        this.unsupported = true;
        this.error = null;
      } else {
        this.error = friendlySettingsError(message, "Shared mode");
      }
    } finally {
      this.loading = false;
    }
  }

  async setMode(mode: "personal" | "shared") {
    if (!isTauri()) {
      this.error = "Shared mode requires the Medousa desktop app";
      return;
    }
    if (this.unsupported && mode === "shared") {
      this.error = "Shared mode isn't available on this workshop yet.";
      return;
    }
    this.saving = true;
    this.error = null;
    try {
      const status = await setSharedMode(mode);
      this.applyStatus(status);
      await userProfiles.load({ suppressRemoteNotice: true });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (isMissingCapabilityError(message)) {
        this.unsupported = true;
        this.mode = "personal";
      }
      this.error = friendlySettingsError(message, "Shared mode");
    } finally {
      this.saving = false;
    }
  }
}

export const sharedMode = new SharedModeStore();
