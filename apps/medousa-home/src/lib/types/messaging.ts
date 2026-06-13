export type ChannelId = "telegram" | "discord" | "slack" | "whatsapp";

export type ChannelStatus = "connected" | "ready" | "needs_setup";

export interface TelegramChannelSummary {
  allowedUserIds: number[];
  heartbeatNudgesEnabled: boolean;
  heartbeatChatIds: number[];
  credentialsSet: boolean;
  adapterRunning: boolean;
}

export interface DiscordChannelSummary {
  commandPrefix: string;
  heartbeatNudgesEnabled: boolean;
  heartbeatChannelIds: number[];
  credentialsSet: boolean;
  adapterRunning: boolean;
}

export interface SlackChannelSummary {
  allowedUserIds: string[];
  heartbeatNudgesEnabled: boolean;
  heartbeatChannelIds: string[];
  botTokenSet: boolean;
  appTokenSet: boolean;
  adapterRunning: boolean;
}

export interface WhatsAppChannelSummary {
  deliverBind: string;
  deliverUrl?: string | null;
  sessionDbPath?: string | null;
  allowedUserIds: string[];
  heartbeatNudgesEnabled: boolean;
  heartbeatChatJids: string[];
  adapterRunning: boolean;
}

export interface ProductConfigSummary {
  telegram: TelegramChannelSummary;
  discord: DiscordChannelSummary;
  slack: SlackChannelSummary;
  whatsapp: WhatsAppChannelSummary;
}

export type ChannelAccent = "primary" | "secondary" | "tertiary" | "success";

/** Workshop palette slot per channel — lively but theme-native, not competitor brand hex. */
export const CHANNEL_ACCENT: Record<ChannelId, ChannelAccent> = {
  telegram: "tertiary",
  discord: "secondary",
  slack: "primary",
  whatsapp: "success",
};

export function channelIconClasses(id: ChannelId, large = false): string {
  const accent = CHANNEL_ACCENT[id];
  return [
    "messaging-channel-icon",
    `messaging-channel-icon-${accent}`,
    large ? "messaging-channel-icon-lg" : "",
  ]
    .filter(Boolean)
    .join(" ");
}

export function channelCredentialsInsetClass(id: ChannelId): string {
  return `messaging-credentials-inset messaging-credentials-inset-${CHANNEL_ACCENT[id]}`;
}

export interface ChannelMeta {
  id: ChannelId;
  name: string;
  description: string;
  tagline: string;
  credentialsTitle: string;
  credentialsBlurb: string;
  setupGuideUrl?: string;
  setupGuideLabel?: string;
  secretIds?: string[];
}

export const MESSAGING_CHANNELS: ChannelMeta[] = [
  {
    id: "telegram",
    name: "Telegram",
    description: "DMs, groups, and topics",
    tagline: "Run Medousa from Telegram DMs, groups, and topics.",
    credentialsTitle: "Get your credentials",
    credentialsBlurb:
      "Create a bot with BotFather, copy the token, and paste it below. Restrict who can reach Medousa with allowed user IDs.",
    setupGuideUrl: "https://core.telegram.org/bots#6-botfather",
    setupGuideLabel: "Open Telegram BotFather guide",
    secretIds: ["telegram_bot_token"],
  },
  {
    id: "discord",
    name: "Discord",
    description: "Prefix commands in servers",
    tagline: "Invite Medousa to Discord servers and reach her with prefix commands.",
    credentialsTitle: "Get your credentials",
    credentialsBlurb:
      "Create a Discord application, add a bot, and paste the bot token. Set a command prefix your server will recognize.",
    setupGuideUrl: "https://discord.com/developers/applications",
    setupGuideLabel: "Open Discord Developer Portal",
    secretIds: ["discord_bot_token"],
  },
  {
    id: "slack",
    name: "Slack",
    description: "Socket Mode workspace bot",
    tagline: "Connect Medousa to Slack with Socket Mode — bot and app tokens required.",
    credentialsTitle: "Get your credentials",
    credentialsBlurb:
      "Create a Slack app with Socket Mode enabled. You need both a bot token (xoxb) and an app-level token (xapp).",
    setupGuideUrl: "https://api.slack.com/apps",
    setupGuideLabel: "Open Slack app dashboard",
    secretIds: ["slack_bot_token", "slack_app_token"],
  },
  {
    id: "whatsapp",
    name: "WhatsApp",
    description: "Bridge deliver bind",
    tagline: "Reach Medousa through a WhatsApp bridge — no bot token, bind address and allowed JIDs.",
    credentialsTitle: "Bridge setup",
    credentialsBlurb:
      "Point Medousa at your bridge deliver bind and list the JIDs allowed to message the workshop.",
    setupGuideLabel: "See bridge docs in product_config",
  },
];

export function channelMeta(id: ChannelId): ChannelMeta {
  return MESSAGING_CHANNELS.find((channel) => channel.id === id) ?? MESSAGING_CHANNELS[0];
}
