import type {
  ChannelId,
  ChannelStatus,
  ProductConfigSummary,
} from "$lib/types/messaging";
import type { LivelinessVariant } from "$lib/types/liveliness";

export function channelStatus(
  channelId: ChannelId,
  summary: ProductConfigSummary | null,
  daemonOk: boolean,
): ChannelStatus {
  if (!summary) return "needs_setup";

  switch (channelId) {
    case "telegram": {
      const channel = summary.telegram;
      if (!channel.credentialsSet || channel.allowedUserIds.length === 0) {
        return "needs_setup";
      }
      if (channel.adapterRunning && daemonOk) return "connected";
      return "ready";
    }
    case "discord": {
      const channel = summary.discord;
      if (!channel.credentialsSet) return "needs_setup";
      if (channel.adapterRunning && daemonOk) return "connected";
      return "ready";
    }
    case "slack": {
      const channel = summary.slack;
      if (
        !channel.botTokenSet ||
        !channel.appTokenSet ||
        channel.allowedUserIds.length === 0
      ) {
        return "needs_setup";
      }
      if (channel.adapterRunning && daemonOk) return "connected";
      return "ready";
    }
    case "whatsapp": {
      const channel = summary.whatsapp;
      if (!channel.deliverBind.trim() || channel.allowedUserIds.length === 0) {
        return "needs_setup";
      }
      if (channel.adapterRunning && daemonOk) return "connected";
      return "ready";
    }
    default:
      return "needs_setup";
  }
}

export function statusLabel(status: ChannelStatus): string {
  switch (status) {
    case "connected":
      return "Live";
    case "ready":
      return "Configured";
    default:
      return "Needs setup";
  }
}

export function statusClass(status: ChannelStatus): string {
  switch (status) {
    case "connected":
    case "ready":
      return "text-primary-300";
    default:
      return "text-surface-500";
  }
}

export function statusDotClass(status: ChannelStatus): string {
  switch (status) {
    case "connected":
      return "messaging-status-dot messaging-status-dot-live";
    case "ready":
      return "messaging-status-dot messaging-status-dot-ready";
    default:
      return "messaging-status-dot messaging-status-dot-setup";
  }
}

export function statusChipVariant(status: ChannelStatus): LivelinessVariant {
  switch (status) {
    case "connected":
      return "live";
    case "ready":
      return "ready";
    default:
      return "setup";
  }
}

export function statusChipClass(status: ChannelStatus): string {
  switch (status) {
    case "connected":
      return "workshop-liveliness-chip workshop-liveliness-chip-live";
    case "ready":
      return "workshop-liveliness-chip workshop-liveliness-chip-ready";
    default:
      return "workshop-liveliness-chip workshop-liveliness-chip-setup";
  }
}
