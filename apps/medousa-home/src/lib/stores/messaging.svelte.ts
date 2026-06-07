import {
  loadProductConfigSummary,
  messagingClearSecret,
  messagingSaveSecret,
  saveDiscordConfig,
  saveSlackConfig,
  saveTelegramConfig,
  saveWhatsAppConfig,
} from "$lib/messaging";
import type { ProductConfigSummary } from "$lib/types/messaging";

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
      this.error = err instanceof Error ? err.message : String(err);
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
      if (config.clearToken) {
        await messagingClearSecret("telegram_bot_token");
      } else if (config.botToken?.trim()) {
        await messagingSaveSecret("telegram_bot_token", config.botToken);
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
      if (config.clearToken) {
        await messagingClearSecret("discord_bot_token");
      } else if (config.botToken?.trim()) {
        await messagingSaveSecret("discord_bot_token", config.botToken);
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
      if (config.clearBotToken) {
        await messagingClearSecret("slack_bot_token");
      } else if (config.botToken?.trim()) {
        await messagingSaveSecret("slack_bot_token", config.botToken);
      }
      if (config.clearAppToken) {
        await messagingClearSecret("slack_app_token");
      } else if (config.appToken?.trim()) {
        await messagingSaveSecret("slack_app_token", config.appToken);
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
