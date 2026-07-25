import { getSharedMode, setSharedMode, type SharedModeStatus } from "$lib/daemon";
import { isTauri } from "$lib/window";
import { userProfiles } from "$lib/stores/userProfiles.svelte";

export class SharedModeStore {
  mode = $state<"personal" | "shared">("personal");
  rootProfileId = $state("user:root");
  generalProfileId = $state("user:general");
  enabledAt = $state<string | null>(null);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);

  isShared = $derived(this.mode === "shared");

  applyStatus(status: SharedModeStatus) {
    this.mode = status.mode === "shared" ? "shared" : "personal";
    this.rootProfileId = status.root_profile_id;
    this.generalProfileId = status.general_profile_id;
    this.enabledAt = status.enabled_at ?? null;
  }

  async load() {
    if (!isTauri()) return;
    this.loading = true;
    this.error = null;
    try {
      const status = await getSharedMode();
      this.applyStatus(status);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  async setMode(mode: "personal" | "shared") {
    if (!isTauri()) {
      this.error = "Shared mode requires the Medousa desktop app";
      return;
    }
    this.saving = true;
    this.error = null;
    try {
      const status = await setSharedMode(mode);
      this.applyStatus(status);
      await userProfiles.load({ suppressRemoteNotice: true });
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.saving = false;
    }
  }
}

export const sharedMode = new SharedModeStore();
