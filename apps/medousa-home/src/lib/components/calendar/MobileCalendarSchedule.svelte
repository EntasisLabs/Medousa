<script lang="ts">
  import { Plus } from "@lucide/svelte";
  import {
    MOBILE_CALENDAR_SCHEDULE_DAYS,
    calendar,
    calendarDateUtils,
  } from "$lib/stores/calendar.svelte";
  import type { CalendarEvent, CalendarReminder } from "$lib/types/calendar";

  const { addDays, startOfDay, isoDay } = calendarDateUtils;

  type ScheduleBucket = {
    day: Date;
    events: CalendarEvent[];
    reminders: CalendarReminder[];
  };

  function eventMatches(event: CalendarEvent, query: string): boolean {
    if (!query) return true;
    return [event.summary, event.location ?? "", event.description ?? ""]
      .some((value) => value.toLocaleLowerCase().includes(query));
  }

  function reminderMatches(reminder: CalendarReminder, query: string): boolean {
    return !query || reminder.title.toLocaleLowerCase().includes(query);
  }

  const buckets = $derived.by((): ScheduleBucket[] => {
    const query = calendar.railQuery.trim().toLocaleLowerCase();
    const start = startOfDay(calendar.selectedDay);
    const result: ScheduleBucket[] = [];
    for (let offset = 0; offset < MOBILE_CALENDAR_SCHEDULE_DAYS; offset += 1) {
      const day = addDays(start, offset);
      const events = calendar
        .eventsForDay(day)
        .filter((event) => eventMatches(event, query))
        .sort((left, right) => {
          if (left.all_day !== right.all_day) return left.all_day ? -1 : 1;
          return new Date(left.dtstart).getTime() - new Date(right.dtstart).getTime();
        });
      const reminders = calendar
        .remindersForDay(day)
        .filter((reminder) => reminderMatches(reminder, query));
      if (events.length > 0 || reminders.length > 0) {
        result.push({ day, events, reminders });
      }
    }
    return result;
  });

  function dayLabel(day: Date): string {
    const today = startOfDay(new Date());
    const tomorrow = addDays(today, 1);
    const date = day.toLocaleDateString(undefined, { month: "short", day: "numeric" });
    if (isoDay(day) === isoDay(today)) return `Today — ${date}`;
    if (isoDay(day) === isoDay(tomorrow)) return `Tomorrow — ${date}`;
    return day.toLocaleDateString(undefined, {
      weekday: "long",
      month: "short",
      day: "numeric",
    });
  }

  function isToday(day: Date): boolean {
    return isoDay(day) === isoDay(new Date());
  }

  function eventTimes(event: CalendarEvent): { start: string; end: string } {
    if (event.all_day) return { start: "All day", end: "" };
    const options: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };
    return {
      start: new Date(event.dtstart).toLocaleTimeString(undefined, options),
      end: event.dtend
        ? new Date(event.dtend).toLocaleTimeString(undefined, options)
        : "",
    };
  }
</script>

<div class="mobile-calendar-schedule">
  {#if buckets.length === 0}
    <div class="mobile-calendar-schedule-empty">
      <p>{calendar.railQuery.trim() ? "No matching events" : "Nothing coming up"}</p>
      <span>
        {calendar.railQuery.trim()
          ? "Try a different title or location."
          : "Your next events and reminders will appear here."}
      </span>
      {#if !calendar.railQuery.trim()}
        <button type="button" onclick={() => calendar.openCreate(calendar.selectedDay)}>
          <Plus size={16} strokeWidth={2} />
          Add something
        </button>
      {/if}
    </div>
  {:else}
    {#each buckets as bucket (isoDay(bucket.day))}
      <section class="mobile-calendar-day-group">
        <h2 class:mobile-calendar-day-today={isToday(bucket.day)}>
          {dayLabel(bucket.day)}
        </h2>
        <div class="mobile-calendar-day-items">
          {#each bucket.reminders as reminder (reminder.id)}
            <button
              type="button"
              class="mobile-calendar-schedule-row mobile-calendar-reminder-row"
              onclick={() => void calendar.completeReminder(reminder, true)}
            >
              <i class="mobile-calendar-reminder-mark" aria-hidden="true"></i>
              <span class="mobile-calendar-schedule-copy">
                <strong>{reminder.title}</strong>
                <small>Reminder · tap to complete</small>
              </span>
              <span class="mobile-calendar-schedule-time">Due</span>
            </button>
          {/each}
          {#each bucket.events as event (`${event.uid}:${event.recurrence_id ?? event.dtstart}`)}
            {@const times = eventTimes(event)}
            <button
              type="button"
              class="mobile-calendar-schedule-row"
              onclick={() => calendar.openEdit(event)}
            >
              <i class="mobile-calendar-event-mark" aria-hidden="true"></i>
              <span class="mobile-calendar-schedule-copy">
                <strong>{event.summary}</strong>
                {#if event.location}
                  <small>{event.location}</small>
                {/if}
              </span>
              <span class="mobile-calendar-schedule-time">
                <span>{times.start}</span>
                {#if times.end}<small>{times.end}</small>{/if}
              </span>
            </button>
          {/each}
        </div>
      </section>
    {/each}
  {/if}
</div>

<style>
  .mobile-calendar-schedule {
    min-height: 100%;
    padding: 0.35rem 1rem 5.25rem;
  }

  .mobile-calendar-day-group + .mobile-calendar-day-group {
    margin-top: 1.35rem;
  }

  .mobile-calendar-day-group h2 {
    margin: 0;
    border-bottom: 1px solid rgb(var(--shell-border) / 0.58);
    padding: 0.55rem 0.15rem 0.6rem;
    font-size: 1rem;
    font-weight: 650;
    letter-spacing: -0.012em;
    color: rgb(var(--theme-text));
  }

  .mobile-calendar-day-group h2.mobile-calendar-day-today {
    color: rgb(var(--theme-error));
  }

  .mobile-calendar-day-items {
    display: flex;
    flex-direction: column;
  }

  .mobile-calendar-schedule-row {
    display: grid;
    grid-template-columns: 0.25rem minmax(0, 1fr) auto;
    min-height: 4rem;
    align-items: center;
    gap: 0.7rem;
    width: 100%;
    border-bottom: 1px solid rgb(var(--shell-border) / 0.32);
    padding: 0.62rem 0.3rem;
    text-align: left;
  }

  .mobile-calendar-schedule-row:active {
    background: rgb(var(--color-surface-800) / 0.45);
  }

  .mobile-calendar-event-mark {
    width: 0.22rem;
    height: 2.5rem;
    border-radius: 9999px;
    background: rgb(var(--color-primary-400));
  }

  .mobile-calendar-reminder-mark {
    width: 0.9rem;
    height: 0.9rem;
    margin-left: -0.3rem;
    border: 1.5px solid rgb(var(--color-secondary-300) / 0.9);
    border-radius: 0.26rem;
    background: transparent;
  }

  .mobile-calendar-schedule-copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 0.18rem;
  }

  .mobile-calendar-schedule-copy strong {
    overflow: hidden;
    font-size: 1rem;
    font-weight: 590;
    line-height: 1.25;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: rgb(var(--theme-text));
  }

  .mobile-calendar-schedule-copy small,
  .mobile-calendar-schedule-time small {
    overflow: hidden;
    font-size: 0.75rem;
    line-height: 1.25;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: rgb(var(--shell-muted));
  }

  .mobile-calendar-schedule-time {
    display: flex;
    max-width: 5.75rem;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.15rem;
    font-size: 0.8125rem;
    line-height: 1.2;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--theme-text-secondary));
  }

  .mobile-calendar-schedule-empty {
    display: flex;
    min-height: 20rem;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    padding: 2rem;
    text-align: center;
  }

  .mobile-calendar-schedule-empty p {
    margin: 0;
    font-size: 1rem;
    font-weight: 650;
    color: rgb(var(--theme-text));
  }

  .mobile-calendar-schedule-empty span {
    max-width: 17rem;
    font-size: 0.8125rem;
    line-height: 1.45;
    color: rgb(var(--shell-muted));
  }

  .mobile-calendar-schedule-empty button {
    display: inline-flex;
    min-height: 2.75rem;
    align-items: center;
    gap: 0.4rem;
    margin-top: 0.65rem;
    border-radius: 9999px;
    background: rgb(var(--color-surface-800) / 0.72);
    padding: 0.4rem 0.9rem;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--theme-text));
  }
</style>
