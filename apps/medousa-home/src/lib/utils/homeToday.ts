import type { CalendarEvent } from "$lib/types/calendar";

export type HomeTodayEventRow = {
  event: CalendarEvent;
  title: string;
  timeLabel: string;
  timing: "all-day" | "now" | "upcoming";
};

export type HomeTodayAgenda = {
  rows: HomeTodayEventRow[];
  hiddenCount: number;
};

/**
 * Keep Home's calendar glance useful and quiet: only events that are still
 * relevant today, ordered all-day first and then chronologically.
 */
export function homeTodayAgenda(
  events: CalendarEvent[],
  now = new Date(),
  limit = 3,
): HomeTodayAgenda {
  const remaining = events
    .filter(
      (event) => eventOccursOnDay(event, now) && eventStillMatters(event, now),
    )
    .slice()
    .sort((a, b) => {
      if (a.all_day !== b.all_day) return a.all_day ? -1 : 1;
      return eventStartMs(a) - eventStartMs(b);
    });

  const visible = remaining.slice(0, Math.max(0, limit));
  return {
    rows: visible.map((event) => toHomeTodayRow(event, now)),
    hiddenCount: Math.max(0, remaining.length - visible.length),
  };
}

function eventOccursOnDay(event: CalendarEvent, day: Date): boolean {
  if (event.all_day) {
    const dayKey = localDayKey(day);
    const startKey = calendarDayKey(event.dtstart);
    const endExclusive = event.dtend
      ? calendarDayKey(event.dtend)
      : nextCalendarDayKey(startKey);
    return dayKey >= startKey && dayKey < endExclusive;
  }

  const dayStart = new Date(day);
  dayStart.setHours(0, 0, 0, 0);
  const dayEnd = new Date(dayStart);
  dayEnd.setDate(dayEnd.getDate() + 1);
  const start = eventStartMs(event);
  if (!Number.isFinite(start)) return false;
  if (!event.dtend) return start >= dayStart.getTime() && start < dayEnd.getTime();
  const end = Date.parse(event.dtend);
  return (
    Number.isFinite(end) && start < dayEnd.getTime() && end > dayStart.getTime()
  );
}

function eventStillMatters(event: CalendarEvent, now: Date): boolean {
  if (event.all_day) return true;

  const start = eventStartMs(event);
  if (!Number.isFinite(start)) return false;

  if (!event.dtend) return start >= now.getTime();
  const end = Date.parse(event.dtend);
  return Number.isFinite(end) ? end > now.getTime() : start >= now.getTime();
}

function eventStartMs(event: CalendarEvent): number {
  const start = Date.parse(event.dtstart);
  return Number.isFinite(start) ? start : Number.MAX_SAFE_INTEGER;
}

function localDayKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function calendarDayKey(value: string): string {
  return value.match(/^(\d{4}-\d{2}-\d{2})/)?.[1] ?? localDayKey(new Date(value));
}

function nextCalendarDayKey(dayKey: string): string {
  const date = new Date(`${dayKey}T00:00:00.000Z`);
  date.setUTCDate(date.getUTCDate() + 1);
  return date.toISOString().slice(0, 10);
}

function toHomeTodayRow(event: CalendarEvent, now: Date): HomeTodayEventRow {
  if (event.all_day) {
    return {
      event,
      title: event.summary.trim() || "Untitled event",
      timeLabel: "All day",
      timing: "all-day",
    };
  }

  const start = new Date(event.dtstart);
  const end = event.dtend ? new Date(event.dtend) : null;
  const ongoing =
    start.getTime() <= now.getTime() &&
    end !== null &&
    Number.isFinite(end.getTime()) &&
    end.getTime() > now.getTime();

  return {
    event,
    title: event.summary.trim() || "Untitled event",
    timeLabel: ongoing
      ? "Now"
      : start.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }),
    timing: ongoing ? "now" : "upcoming",
  };
}
