import {
  createUserProfile,
  listUserProfiles,
  setActiveUserProfile,
  type DaemonHealth,
} from "$lib/daemon";
import type { UserProfileRecord } from "$lib/types/userProfile";
import { chat } from "$lib/stores/chat.svelte";
import { identity } from "$lib/stores/identity.svelte";

const PROFILE_SLUG_PATTERN = /^[a-z][a-z0-9_-]{0,31}$/;

export class UserProfilesStore {
  profiles = $state<UserProfileRecord[]>([]);
  activeProfileId = $state<string | null>(null);
  resolvedUserId = $state<string | null>(null);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);
  message = $state<string | null>(null);
  /** After switching profile mid-conversation — nudge to start fresh chat. */
  switchNotice = $state<string | null>(null);
  /** Active profile changed on another device (last-writer-wins). */
  remoteChangeNotice = $state<string | null>(null);
  private localSwitchPending = false;

  activeProfile = $derived(
    this.profiles.find((profile) => profile.profile_id === this.activeProfileId) ?? null,
  );

  activeDisplayName = $derived(this.activeProfile?.display_name ?? "Personal");

  profileMonogram = $derived(
    this.activeDisplayName.trim().charAt(0).toUpperCase() || "P",
  );

  hasMultipleProfiles = $derived(this.profiles.length > 1);

  applyHealthSnapshot(health: DaemonHealth | null) {
    if (!health?.ok || !health.active_profile_id) return;
    if (this.localSwitchPending) return;
    if (this.activeProfileId && health.active_profile_id !== this.activeProfileId) {
      return;
    }
    this.activeProfileId = health.active_profile_id;
  }

  async load(options?: { suppressRemoteNotice?: boolean }) {
    const previousActive = this.activeProfileId;
    this.loading = true;
    this.error = null;
    try {
      const response = await listUserProfiles();
      this.profiles = response.profiles.filter((profile) => !profile.archived);
      this.activeProfileId = response.active_profile_id;
      this.resolvedUserId = response.resolved_user_id;

      if (
        previousActive &&
        response.active_profile_id !== previousActive &&
        !this.localSwitchPending &&
        !options?.suppressRemoteNotice
      ) {
        const remoteProfile = this.profiles.find(
          (profile) => profile.profile_id === response.active_profile_id,
        );
        const label = remoteProfile?.display_name ?? "Another profile";
        this.remoteChangeNotice = `${label} is now active — switched on another device.`;
        await Promise.all([
          identity.refresh({ relationshipLimit: 8 }),
          chat.refreshSessions({ force: true }),
        ]);
      }
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
      this.localSwitchPending = false;
    }
  }

  /** Foreground resume: cheap health hint + full profile sync. */
  async syncOnResume(health?: DaemonHealth | null) {
    this.applyHealthSnapshot(health ?? null);
    await this.load();
  }

  async setActive(profileId: string) {
    if (profileId === this.activeProfileId) return;
    const hadConversation = chat.messages.length > 0;
    this.localSwitchPending = true;
    this.remoteChangeNotice = null;
    this.saving = true;
    this.error = null;
    this.message = null;
    try {
      const response = await setActiveUserProfile(profileId);
      this.activeProfileId = response.active_profile_id;
      this.resolvedUserId = response.resolved_user_id;
      await this.load({ suppressRemoteNotice: true });
      await Promise.all([
        identity.refresh({ relationshipLimit: 8 }),
        chat.refreshSessions({ force: true }),
      ]);
      if (hadConversation) {
        this.switchNotice =
          `${this.activeDisplayName} is active. Work and home stay separate — start a new chat when you switch contexts.`;
      } else {
        this.message = `Switched to ${this.activeDisplayName}`;
      }
    } catch (err) {
      this.localSwitchPending = false;
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.saving = false;
    }
  }

  async create(slug: string, displayName: string) {
    const normalizedSlug = slug.trim().toLowerCase();
    const normalizedName = displayName.trim();
    if (!PROFILE_SLUG_PATTERN.test(normalizedSlug)) {
      this.error =
        "Profile slug must start with a letter and use lowercase letters, numbers, _ or - (max 32).";
      return false;
    }
    if (!normalizedName) {
      this.error = "Display name is required.";
      return false;
    }

    this.saving = true;
    this.error = null;
    this.message = null;
    try {
      await createUserProfile(normalizedSlug, normalizedName);
      await this.load({ suppressRemoteNotice: true });
      this.message = `Created ${normalizedName}`;
      return true;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return false;
    } finally {
      this.saving = false;
    }
  }

  dismissSwitchNotice() {
    this.switchNotice = null;
  }

  dismissRemoteChangeNotice() {
    this.remoteChangeNotice = null;
  }

  clearMessage() {
    this.message = null;
    this.error = null;
  }

  resetForReconnect() {
    this.profiles = [];
    this.activeProfileId = null;
    this.resolvedUserId = null;
    this.error = null;
    this.message = null;
    this.switchNotice = null;
    this.remoteChangeNotice = null;
    this.localSwitchPending = false;
  }
}

export const userProfiles = new UserProfilesStore();
