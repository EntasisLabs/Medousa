/** Schedule local OS notifications for upcoming calendar VALARMs. */

import type { CalendarEvent } from "$lib/types/calendar";
import { ensureNotificationPermission } from "$lib/notifications";

const FIRED_KEY = "medousa-calendar-alarms-fired";
const FIRED_LIMIT = 256;

type FiredMap = Record<string, number>;

function notificationsEnabled(): boolean {
  if (typeof localStorage === "undefined") return true;
  return localStorage.getItem("medousa-home-notifications") !== "0";
}

function loadFired(): FiredMap {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(FIRED_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as FiredMap;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function saveFired(map: FiredMap) {
  if (typeof localStorage === "undefined") return;
  const entries = Object.entries(map).sort((a, b) => b[1] - a[1]).slice(0, FIRED_LIMIT);
  localStorage.setItem(FIRED_KEY, JSON.stringify(Object.fromEntries(entries)));
}

function alarmSeed(event: CalendarEvent, minutesBefore: number, fireAt: number): string {
  return `${event.uid}:${event.recurrence_id ?? event.dtstart}:${minutesBefore}:${fireAt}`;
}

function notificationId(seed: string): number {
  let hash = 0;
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) || 1;
}

async function sendCalendarNotification(seed: string, title: string, body: string, uid: string) {
  if (!notificationsEnabled()) return;
  if (!(await ensureNotificationPermission())) return;
  try {
    const { sendNotification } = await import("@tauri-apps/plugin-notification");
    sendNotification({
      id: notificationId(seed),
      title,
      body,
      actionTypeId: "medousa-calendar",
      extra: { kind: "calendar", uid },
    });
  } catch {
    // Vite-only / plugin unavailable.
  }
}

/** Fire any due alarms in the loaded event window. Safe to call on an interval. */
export async function pollCalendarAlarms(events: CalendarEvent[]): Promise<void> {
  const now = Date.now();
  const fired = loadFired();
  let dirty = false;

  for (const event of events) {
    const alarms = event.alarms ?? [];
    if (alarms.length === 0) continue;
    const startMs = new Date(event.dtstart).getTime();
    if (!Number.isFinite(startMs)) continue;

    for (const alarm of alarms) {
      const minutes = alarm.trigger_minutes_before;
      if (!minutes || minutes <= 0) continue;
      const fireAt = startMs - minutes * 60_000;
      // Fire in a short window after the trigger so a missed poll still delivers.
      if (now < fireAt || now > fireAt + 15 * 60_000) continue;
      const seed = alarmSeed(event, minutes, fireAt);
      if (fired[seed]) continue;
      fired[seed] = now;
      dirty = true;
      const when =
        minutes >= 1440
          ? `${Math.round(minutes / 1440)}d`
          : minutes >= 60
            ? `${Math.round(minutes / 60)}h`
            : `${minutes}m`;
      await sendCalendarNotification(
        seed,
        "Medousa — upcoming",
        `${event.summary} · in ${when}`,
        event.uid,
      );
    }
  }

  if (dirty) saveFired(fired);
}
