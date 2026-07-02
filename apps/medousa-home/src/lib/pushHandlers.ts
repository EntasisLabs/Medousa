import { parseDeepLink } from "$lib/deepLinks";
import type { LiveActivityPayload } from "$lib/liveActivity";
import { syncHomeWidget } from "$lib/homeWidget";
import type { OpenWorkHandler } from "$lib/mobileNative";
import { isTauriIos } from "$lib/platform";

function readString(data: Record<string, unknown>, key: string): string | undefined {
  const value = data[key];
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed ? trimmed : undefined;
  }
  if (typeof value === "number" && Number.isFinite(value)) {
    return String(value);
  }
  return undefined;
}

function parsePulsePayload(raw: unknown): LiveActivityPayload | null {
  if (typeof raw !== "string" || !raw.trim()) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<LiveActivityPayload>;
    if (!parsed.mood || !parsed.eyebrow || !parsed.headline) return null;
    return {
      mood: parsed.mood as LiveActivityPayload["mood"],
      workshopName: parsed.workshopName?.trim() || "Workshop",
      eyebrow: parsed.eyebrow,
      headline: parsed.headline,
      subline: parsed.subline,
      motionSummary: parsed.motionSummary,
      blockedCount: parsed.blockedCount ?? 0,
      primaryCardId: parsed.primaryCardId,
    };
  } catch {
    return null;
  }
}

async function applyPulseNotification(data: Record<string, unknown>): Promise<void> {
  const fromJson = parsePulsePayload(data.medousaPulse);
  if (fromJson) {
    await syncHomeWidget(fromJson, { force: true });
    return;
  }

  if (readString(data, "medousaType") !== "pulse_snapshot") return;

  const payload: LiveActivityPayload = {
    mood: (readString(data, "mood") as LiveActivityPayload["mood"]) ?? "quiet",
    workshopName: readString(data, "workshopName") ?? "Workshop",
    eyebrow: readString(data, "eyebrow") ?? "Quiet",
    headline: readString(data, "headline") ?? "Medousa",
    subline: readString(data, "subline"),
    motionSummary: readString(data, "motionSummary"),
    blockedCount: Number(readString(data, "blockedCount") ?? "0") || 0,
    primaryCardId: readString(data, "primaryCardId"),
  };
  await syncHomeWidget(payload, { force: true });
}

function openWorkFromNotification(
  data: Record<string, unknown>,
  onOpenWork: OpenWorkHandler,
): void {
  const url = readString(data, "url");
  if (url) {
    const link = parseDeepLink(url);
    if (link?.kind === "work") {
      void onOpenWork(link.cardId);
      return;
    }
  }

  const cardId = readString(data, "cardId") ?? readString(data, "card_id");
  if (cardId) {
    void onOpenWork(cardId);
  }
}

/** Remote APNs: widget pulse refresh + tap-to-open work cards. */
export async function initRemotePushHandlers(
  onOpenWork: OpenWorkHandler,
): Promise<(() => void) | null> {
  if (!isTauriIos()) return null;

  try {
    const { onNotificationReceived, onNotificationTapped } = await import(
      "tauri-plugin-mobile-push-api"
    );

    const received = await onNotificationReceived((notification) => {
      const data = (notification.data ?? {}) as Record<string, unknown>;
      void applyPulseNotification(data);
    });

    const tapped = await onNotificationTapped((notification) => {
      const data = (notification.data ?? {}) as Record<string, unknown>;
      openWorkFromNotification(data, onOpenWork);
    });

    return () => {
      void received.unregister();
      void tapped.unregister();
    };
  } catch (err) {
    console.warn("[mobile-push] remote handlers unavailable:", err);
    return null;
  }
}
