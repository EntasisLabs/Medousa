import {
  createCalendarEvent,
  deleteCalendarEvent,
  exportCalendar,
  importCalendarIcs,
  listCalendarEvents,
  updateCalendarEvent,
} from "$lib/daemon";
import type { CalendarEvent, CalendarWriteRequest } from "$lib/types/calendar";

export type CalendarViewMode = "month" | "week" | "day";

function startOfDay(date: Date): Date {
  const next = new Date(date);
  next.setHours(0, 0, 0, 0);
  return next;
}

function addDays(date: Date, days: number): Date {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
}

function startOfWeek(date: Date): Date {
  const next = startOfDay(date);
  const day = next.getDay();
  const mondayOffset = day === 0 ? -6 : 1 - day;
  return addDays(next, mondayOffset);
}

function startOfMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function endOfMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth() + 1, 0, 23, 59, 59, 999);
}

function isoDay(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** Calendar date for all-day events (DATE values are timezone-agnostic). */
function allDayKey(iso: string): string {
  const match = iso.match(/^(\d{4}-\d{2}-\d{2})/);
  if (match) return match[1];
  const dt = new Date(iso);
  const y = dt.getUTCFullYear();
  const m = String(dt.getUTCMonth() + 1).padStart(2, "0");
  const d = String(dt.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** Next calendar day as YYYY-MM-DD (UTC date arithmetic). */
function nextAllDayKey(dayKey: string): string {
  const dt = new Date(`${dayKey}T00:00:00.000Z`);
  dt.setUTCDate(dt.getUTCDate() + 1);
  return allDayKey(dt.toISOString());
}

/** Encode an all-day bound as UTC midnight of that calendar date. */
function allDayBoundIso(dayKey: string): string {
  return `${dayKey}T00:00:00.000Z`;
}

class CalendarStore {
  viewMode = $state<CalendarViewMode>("month");
  anchor = $state<Date>(startOfDay(new Date()));
  selectedDay = $state<Date>(startOfDay(new Date()));
  events = $state<CalendarEvent[]>([]);
  calendarPath = $state("calendar/personal.ics");
  loading = $state(false);
  error = $state<string | null>(null);
  notice = $state<string | null>(null);
  editorOpen = $state(false);
  editing = $state<CalendarEvent | null>(null);

  rangeForView(): { from: Date; to: Date } {
    if (this.viewMode === "day") {
      const from = startOfDay(this.selectedDay);
      return { from, to: addDays(from, 1) };
    }
    if (this.viewMode === "week") {
      const from = startOfWeek(this.selectedDay);
      return { from, to: addDays(from, 7) };
    }
    const monthStart = startOfMonth(this.anchor);
    const gridStart = startOfWeek(monthStart);
    const monthEnd = endOfMonth(this.anchor);
    const gridEnd = addDays(startOfWeek(monthEnd), 7);
    return { from: gridStart, to: gridEnd };
  }

  async refresh() {
    this.loading = true;
    this.error = null;
    try {
      const { from, to } = this.rangeForView();
      // Pad by a day so all-day DATE values (UTC midnight) aren't clipped by local TZ.
      const response = await listCalendarEvents({
        from: addDays(from, -1).toISOString(),
        to: addDays(to, 1).toISOString(),
        path: this.calendarPath,
      });
      this.events = response.events;
      this.calendarPath = response.calendar_path;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  setViewMode(mode: CalendarViewMode) {
    this.viewMode = mode;
    void this.refresh();
  }

  selectDay(date: Date) {
    this.selectedDay = startOfDay(date);
    if (this.viewMode === "month") {
      this.anchor = startOfDay(date);
    }
  }

  shift(delta: number) {
    if (this.viewMode === "month") {
      this.anchor = new Date(this.anchor.getFullYear(), this.anchor.getMonth() + delta, 1);
      this.selectedDay = startOfDay(this.anchor);
    } else if (this.viewMode === "week") {
      this.selectedDay = addDays(this.selectedDay, delta * 7);
      this.anchor = this.selectedDay;
    } else {
      this.selectedDay = addDays(this.selectedDay, delta);
      this.anchor = this.selectedDay;
    }
    void this.refresh();
  }

  goToday() {
    const today = startOfDay(new Date());
    this.anchor = today;
    this.selectedDay = today;
    void this.refresh();
  }

  eventsForDay(date: Date): CalendarEvent[] {
    const day = isoDay(date);
    return this.events.filter((event) => {
      if (event.all_day) {
        const start = allDayKey(event.dtstart);
        // RFC 5545 all-day DTEND is exclusive.
        const endExclusive = event.dtend ? allDayKey(event.dtend) : nextAllDayKey(start);
        return day >= start && day < endExclusive;
      }
      const start = isoDay(new Date(event.dtstart));
      const end = event.dtend ? isoDay(new Date(event.dtend)) : start;
      // Timed events spanning midnight: include each local day they touch.
      if (!event.dtend) return day === start;
      const endDay = end;
      const endMs = new Date(event.dtend).getTime();
      const endDayStart = new Date(event.dtend);
      endDayStart.setHours(0, 0, 0, 0);
      // If event ends exactly at local midnight, exclude that day.
      if (endMs === endDayStart.getTime()) {
        return day >= start && day < endDay;
      }
      return day >= start && day <= endDay;
    });
  }

  openCreate(day?: Date) {
    this.editing = null;
    if (day) this.selectedDay = startOfDay(day);
    this.editorOpen = true;
  }

  openEdit(event: CalendarEvent) {
    this.editing = event;
    this.editorOpen = true;
  }

  closeEditor() {
    this.editorOpen = false;
    this.editing = null;
  }

  async saveEvent(request: CalendarWriteRequest) {
    const payload = {
      ...request,
      calendar_path: request.calendar_path ?? this.calendarPath,
    };
    if (this.editing) {
      await updateCalendarEvent(this.editing.uid, payload);
    } else {
      await createCalendarEvent(payload);
    }
    this.closeEditor();
    await this.refresh();
  }

  async removeEvent(uid: string) {
    await deleteCalendarEvent(uid, this.calendarPath);
    this.closeEditor();
    await this.refresh();
  }

  async importIcs(ics: string) {
    this.notice = null;
    const result = await importCalendarIcs(ics, this.calendarPath);
    const skipped = result.skipped ?? 0;
    const parts = [
      `Imported ${result.imported}`,
      `updated ${result.updated}`,
      `skipped ${skipped}`,
    ];
    this.notice = parts.join(", ");
    if (result.warnings?.length) {
      this.notice = `${this.notice}. ${result.warnings.slice(0, 3).join("; ")}`;
    }
    await this.refresh();
  }

  async exportIcs(): Promise<string> {
    const response = await exportCalendar(this.calendarPath);
    return response.ics;
  }
}

export const calendar = new CalendarStore();

export const calendarDateUtils = {
  startOfDay,
  addDays,
  startOfWeek,
  startOfMonth,
  endOfMonth,
  isoDay,
  allDayKey,
  nextAllDayKey,
  allDayBoundIso,
};
