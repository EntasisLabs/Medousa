import {
  loadProductConfigSummary,
  messagingSyncAdapters,
  saveDiscordConfig,
  saveSlackConfig,
  saveTelegramConfig,
  saveWhatsAppConfig,
} from "$lib/messaging";
import type { ProductConfigSummary } from "$lib/types/messaging";
import {
  deleteIntegrationSecret,
  ensureConnection,
  putIntegrationSecret,
} from "$lib/utils/integrations";
import {
  friendlySettingsError,
  isMissingCapabilityError,
} from "$lib/utils/normieErrors";

export class MessagingStore {
  summary = $state<ProductConfigSummary | null>(null);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);
  saveMessage = $state<string | null>(null);

  async refresh() {
    this.loading = true;
    this.error = null;
    try {
      this.summary = await loadProductConfigSummary();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      // Quiet when the workshop hasn't exposed product config yet.
      this.error = isMissingCapabilityError(message)
        ? null
        : friendlySettingsError(message, "Channels");
    } finally {
      this.loading = false;
    }
  }

  async saveTelegram(config: {
    allowedUserIds: number[];
    heartbeatNudgesEnabled: boolean;
    heartbeatChatIds: number[];
    botToken?: string | null;
    clearToken?: boolean;
  }) {
    await this.persist(async () => {
      const connection = await ensureConnection("telegram");
      if (config.clearToken) {
        await deleteIntegrationSecret(connection.connection_id, "bot_token");
      } else if (config.botToken?.trim()) {
        await putIntegrationSecret(
          connection.connection_id,
          "bot_token",
          config.botToken.trim(),
        );
      }
      await saveTelegramConfig({
        allowedUserIds: config.allowedUserIds,
        heartbeatNudgesEnabled: config.heartbeatNudgesEnabled,
        heartbeatChatIds: config.heartbeatChatIds,
      });
    });
  }

  async saveDiscord(config: {
    commandPrefix: string;
    heartbeatNudgesEnabled: boolean;
    heartbeatChannelIds: number[];
    botToken?: string | null;
    clearToken?: boolean;
  }) {
    await this.persist(async () => {
      const connection = await ensureConnection("discord");
      if (config.clearToken) {
        await deleteIntegrationSecret(connection.connection_id, "bot_token");
      } else if (config.botToken?.trim()) {
        await putIntegrationSecret(
          connection.connection_id,
          "bot_token",
          config.botToken.trim(),
        );
      }
      await saveDiscordConfig({
        commandPrefix: config.commandPrefix,
        heartbeatNudgesEnabled: config.heartbeatNudgesEnabled,
        heartbeatChannelIds: config.heartbeatChannelIds,
      });
    });
  }

  async saveSlack(config: {
    allowedUserIds: string[];
    heartbeatNudgesEnabled: boolean;
    heartbeatChannelIds: string[];
    botToken?: string | null;
    appToken?: string | null;
    clearBotToken?: boolean;
    clearAppToken?: boolean;
  }) {
    await this.persist(async () => {
      const connection = await ensureConnection("slack");
      if (config.clearBotToken) {
        await deleteIntegrationSecret(connection.connection_id, "bot_token");
      } else if (config.botToken?.trim()) {
        await putIntegrationSecret(
          connection.connection_id,
          "bot_token",
          config.botToken.trim(),
        );
      }
      if (config.clearAppToken) {
        await deleteIntegrationSecret(connection.connection_id, "app_token");
      } else if (config.appToken?.trim()) {
        await putIntegrationSecret(
          connection.connection_id,
          "app_token",
          config.appToken.trim(),
        );
      }
      await saveSlackConfig({
        allowedUserIds: config.allowedUserIds,
        heartbeatNudgesEnabled: config.heartbeatNudgesEnabled,
        heartbeatChannelIds: config.heartbeatChannelIds,
      });
    });
  }

  async saveWhatsApp(config: {
    deliverBind: string;
    deliverUrl?: string | null;
    sessionDbPath?: string | null;
    allowedUserIds: string[];
    heartbeatNudgesEnabled: boolean;
    heartbeatChatJids: string[];
  }) {
    await this.persist(async () => {
      await saveWhatsAppConfig(config);
    });
  }

  private async persist(action: () => Promise<void>) {
    this.saving = true;
    this.saveMessage = null;
    try {
      await action();
      await messagingSyncAdapters();
      await this.refresh();
      this.saveMessage = "Saved";
    } catch (err) {
      this.saveMessage = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.saving = false;
    }
  }
}

export const messaging = new MessagingStore();
