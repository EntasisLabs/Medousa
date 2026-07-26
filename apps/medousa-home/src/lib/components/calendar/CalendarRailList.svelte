<script lang="ts">
  import { CalendarDays, ListTodo, Plus } from "@lucide/svelte";
  import { calendar, calendarDateUtils } from "$lib/stores/calendar.svelte";
  import type { CalendarEvent } from "$lib/types/calendar";
  import { onMount } from "svelte";

  interface Props {
    onPickEvent?: (event: CalendarEvent) => void;
    chrome?: "default" | "rail-list";
  }

  let { onPickEvent, chrome = "rail-list" }: Props = $props();

  const { addDays, startOfDay, startOfMonth, startOfWeek, isoDay } = calendarDateUtils;

  onMount(() => {
    void calendar.refresh();
  });

  type DayBucket = { day: Date; label: string; events: CalendarEvent[]; reminderCount: number };

  const miniCells = $derived.by(() => {
    const monthStart = startOfMonth(calendar.anchor);
    const gridStart = startOfWeek(monthStart);
    return Array.from({ length: 42 }, (_, index) => addDays(gridStart, index));
  });

  const buckets = $derived.by((): DayBucket[] => {
    const { from, to } = calendar.rangeForView();
    const out: DayBucket[] = [];
    let cursor = startOfDay(from);
    const end = startOfDay(to);
    while (cursor < end) {
      const events = calendar.eventsForDay(cursor);
      const reminders = calendar.remindersForDay(cursor);
      if (events.length > 0 || reminders.length > 0) {
        out.push({
          day: cursor,
          label: cursor.toLocaleDateString(undefined, {
            weekday: "short",
            month: "short",
            day: "numeric",
          }),
          events,
          reminderCount: reminders.length,
        });
      }
      cursor = addDays(cursor, 1);
      if (out.length >= 14) break;
    }
    return out;
  });

  function timeLabel(event: CalendarEvent): string {
    if (event.all_day) return "All day";
    const start = new Date(event.dtstart);
    return start.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  }

  function pick(event: CalendarEvent) {
    calendar.openEdit(event);
    onPickEvent?.(event);
  }

  function create() {
    calendar.openCreateMenu();
    onPickEvent?.({
      uid: "",
      summary: "",
      dtstart: calendar.selectedDay.toISOString(),
      all_day: false,
      calendar_path: calendar.calendarPath,
    });
  }

  function isSameDay(a: Date, b: Date): boolean {
    return isoDay(a) === isoDay(b);
  }

  function isToday(date: Date): boolean {
    return isSameDay(date, new Date());
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-chrome={chrome}>
  {#if calendar.error}
    <p class="px-3 py-2 text-xs text-warning-400">{calendar.error}</p>
  {/if}

  <div class="cal-mini px-2 pb-2 pt-1">
    <div class="mb-1.5 flex items-center justify-between px-1">
      <p class="text-[11px] font-semibold tracking-wide text-surface-300">
        {calendar.anchor.toLocaleDateString(undefined, { month: "long", year: "numeric" })}
      </p>
      <button
        type="button"
        class="text-[11px] font-medium text-primary-300 hover:text-primary-200"
        onclick={() => calendar.goToday()}
      >
        Today
      </button>
    </div>
    <div class="cal-mini-weekdays">
      {#each ["S", "M", "T", "W", "T", "F", "S"] as label}
        <span>{label}</span>
      {/each}
    </div>
    <div class="cal-mini-grid">
      {#each miniCells as day (isoDay(day))}
        {@const inMonth = day.getMonth() === calendar.anchor.getMonth()}
        {@const selected = isSameDay(day, calendar.selectedDay)}
        {@const today = isToday(day)}
        {@const hasEvents =
          calendar.eventsForDay(day).length > 0 || calendar.remindersForDay(day).length > 0}
        <button
          type="button"
          class="cal-mini-day"
          class:cal-mini-day-out={!inMonth}
          class:cal-mini-day-selected={selected}
          class:cal-mini-day-today={today}
          class:cal-mini-day-busy={hasEvents}
          onclick={() => {
            calendar.selectDay(day);
            if (calendar.viewMode === "month") {
              calendar.anchor = day;
            }
            void calendar.refresh();
          }}
        >
          {day.getDate()}
        </button>
      {/each}
    </div>
  </div>

  {#if buckets.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 px-3 py-6 text-center">
      <CalendarDays size={22} strokeWidth={1.5} class="text-surface-500" />
      <p class="text-sm text-surface-300">No events in this range</p>
      <button type="button" class="btn btn-sm btn-primary" onclick={create}>
        <Plus size={14} strokeWidth={2} />
        New
      </button>
    </div>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5">
      {#each buckets as bucket (isoDay(bucket.day))}
        <section class="mb-2">
          <p class="px-2 pb-0.5 pt-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
            {bucket.label}
          </p>
          <ul>
            {#if bucket.reminderCount > 0}
              <li>
                <button
                  type="button"
                  class="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-surface-800/70"
                  onclick={() => {
                    calendar.selectDay(bucket.day);
                    calendar.openCreateReminder(bucket.day);
                  }}
                >
                  <span class="w-14 shrink-0 pt-0.5 text-[11px] text-secondary-400">Due</span>
                  <ListTodo size={13} strokeWidth={1.75} class="mt-0.5 shrink-0 text-secondary-400" />
                  <span class="min-w-0 flex-1 truncate text-[13px] text-surface-100">
                    {bucket.reminderCount} reminder{bucket.reminderCount === 1 ? "" : "s"}
                  </span>
                </button>
              </li>
            {/if}
            {#each bucket.events as event (event.uid + (event.recurrence_id ?? ""))}
              <li>
                <button
                  type="button"
                  class="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-surface-800/70"
                  onclick={() => pick(event)}
                >
                  <span class="w-14 shrink-0 pt-0.5 text-[11px] text-surface-500">
                    {timeLabel(event)}
                  </span>
                  <span class="min-w-0 flex-1 truncate text-[13px] font-medium text-surface-100">
                    {event.summary || "Untitled"}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .cal-mini-weekdays,
  .cal-mini-grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 0.1rem;
  }

  .cal-mini-weekdays span {
    text-align: center;
    font-size: 0.625rem;
    font-weight: 600;
    color: rgb(var(--color-surface-500));
  }

  .cal-mini-day {
    aspect-ratio: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 9999px;
    font-size: 0.6875rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-surface-200));
  }

  .cal-mini-day:hover {
    background: rgb(var(--color-surface-800) / 0.8);
  }

  .cal-mini-day-out {
    color: rgb(var(--color-surface-600));
  }

  .cal-mini-day-today {
    box-shadow: inset 0 0 0 1px rgb(var(--color-primary-400) / 0.7);
  }

  .cal-mini-day-selected {
    background: rgb(var(--color-primary-500));
    color: rgb(var(--color-surface-50));
  }

  .cal-mini-day-busy:not(.cal-mini-day-selected) {
    font-weight: 700;
    color: rgb(var(--color-primary-200));
  }
</style>
