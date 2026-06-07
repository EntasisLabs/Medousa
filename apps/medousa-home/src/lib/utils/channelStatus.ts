import type {
  ChannelId,
  ChannelStatus,
  ProductConfigSummary,
} from "$lib/types/messaging";

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
      return daemonOk ? "connected" : "ready";
    }
    case "discord": {
      const channel = summary.discord;
      if (!channel.credentialsSet) return "needs_setup";
      return daemonOk ? "connected" : "ready";
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
      return daemonOk ? "connected" : "ready";
    }
    case "whatsapp": {
      const channel = summary.whatsapp;
      if (
        !channel.deliverBind.trim() ||
        channel.allowedUserIds.length === 0
      ) {
        return "needs_setup";
      }
      return daemonOk ? "connected" : "ready";
    }
    default:
      return "needs_setup";
  }
}

export function statusLabel(status: ChannelStatus): string {
  switch (status) {
    case "connected":
      return "Connected";
    case "ready":
      return "Ready";
    default:
      return "Needs setup";
  }
}

export function statusClass(status: ChannelStatus): string {
  switch (status) {
    case "connected":
      return "text-success-400";
    case "ready":
      return "text-primary-300";
    default:
      return "text-surface-500";
  }
}
