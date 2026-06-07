export type ChannelId = "telegram" | "discord" | "slack" | "whatsapp";

export type ChannelStatus = "connected" | "ready" | "needs_setup";

export interface TelegramChannelSummary {
  allowedUserIds: number[];
  heartbeatNudgesEnabled: boolean;
  heartbeatChatIds: number[];
  credentialsSet: boolean;
}

export interface DiscordChannelSummary {
  commandPrefix: string;
  heartbeatNudgesEnabled: boolean;
  heartbeatChannelIds: number[];
  credentialsSet: boolean;
}

export interface SlackChannelSummary {
  allowedUserIds: string[];
  heartbeatNudgesEnabled: boolean;
  heartbeatChannelIds: string[];
  botTokenSet: boolean;
  appTokenSet: boolean;
}

export interface WhatsAppChannelSummary {
  deliverBind: string;
  deliverUrl?: string | null;
  sessionDbPath?: string | null;
  allowedUserIds: string[];
  heartbeatNudgesEnabled: boolean;
  heartbeatChatJids: string[];
}

export interface ProductConfigSummary {
  telegram: TelegramChannelSummary;
  discord: DiscordChannelSummary;
  slack: SlackChannelSummary;
  whatsapp: WhatsAppChannelSummary;
}

export interface ChannelMeta {
  id: ChannelId;
  name: string;
  description: string;
  secretIds?: string[];
}

export const MESSAGING_CHANNELS: ChannelMeta[] = [
  {
    id: "telegram",
    name: "Telegram",
    description: "Bot ingest and heartbeat nudges",
    secretIds: ["telegram_bot_token"],
  },
  {
    id: "discord",
    name: "Discord",
    description: "Prefix commands and scheduled delivery",
    secretIds: ["discord_bot_token"],
  },
  {
    id: "slack",
    name: "Slack",
    description: "Socket Mode bot and channel delivery",
    secretIds: ["slack_bot_token", "slack_app_token"],
  },
  {
    id: "whatsapp",
    name: "WhatsApp",
    description: "Bridge deliver bind and allowed JIDs",
  },
];
