<script lang="ts">
  import { calendar, calendarDateUtils } from "$lib/stores/calendar.svelte";

  const { addDays, startOfMonth, startOfWeek, isoDay } = calendarDateUtils;
  const weekdayLabels = ["S", "M", "T", "W", "T", "F", "S"];

  let gestureX = 0;
  let gestureY = 0;
  let gesturePointer = -1;
  let suppressClick = false;

  const monthCells = $derived.by(() => {
    const monthStart = startOfMonth(calendar.anchor);
    const gridStart = startOfWeek(monthStart);
    return Array.from({ length: 42 }, (_, index) => addDays(gridStart, index));
  });

  function isSameDay(a: Date, b: Date): boolean {
    return isoDay(a) === isoDay(b);
  }

  function openDay(day: Date) {
    if (suppressClick) return;
    calendar.selectDay(day);
    calendar.setViewMode("schedule");
  }

  function beginGesture(event: PointerEvent) {
    if (!event.isPrimary) return;
    gestureX = event.clientX;
    gestureY = event.clientY;
    gesturePointer = event.pointerId;
    (event.currentTarget as HTMLElement | null)?.setPointerCapture(event.pointerId);
  }

  function finishGesture(event: PointerEvent) {
    if (event.pointerId !== gesturePointer) return;
    const dx = event.clientX - gestureX;
    const dy = event.clientY - gestureY;
    gesturePointer = -1;
    if (Math.abs(dx) < 48 || Math.abs(dx) < Math.abs(dy) * 1.2) return;
    suppressClick = true;
    calendar.shift(dx < 0 ? 1 : -1);
    window.setTimeout(() => (suppressClick = false), 0);
  }
</script>

<div class="mobile-calendar-month">
  <div class="mobile-calendar-weekdays" aria-hidden="true">
    {#each weekdayLabels as label}
      <span>{label}</span>
    {/each}
  </div>

  <div
    class="mobile-calendar-month-grid"
    role="grid"
    aria-label={calendar.anchor.toLocaleDateString(undefined, {
      month: "long",
      year: "numeric",
    })}
    tabindex="0"
    onpointerdown={beginGesture}
    onpointerup={finishGesture}
    onpointercancel={() => (gesturePointer = -1)}
    onkeydown={(event) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      calendar.shift(event.key === "ArrowRight" ? 1 : -1);
    }}
  >
    {#each monthCells as day (isoDay(day))}
      {@const inMonth = day.getMonth() === calendar.anchor.getMonth()}
      {@const selected = isSameDay(day, calendar.selectedDay)}
      {@const today = isSameDay(day, new Date())}
      {@const events = calendar.eventsForDay(day)}
      {@const reminders = calendar.remindersForDay(day)}
      {@const reminderLimit = Math.min(1, reminders.length)}
      {@const eventLimit = Math.max(0, 2 - reminderLimit)}
      {@const visibleCount = reminderLimit + Math.min(eventLimit, events.length)}
      {@const hiddenCount = Math.max(
        0,
        events.length + reminders.length - visibleCount,
      )}
      <button
        type="button"
        role="gridcell"
        class="mobile-calendar-cell"
        class:mobile-calendar-cell-out={!inMonth}
        class:mobile-calendar-cell-selected={selected}
        aria-label={`${day.toLocaleDateString(undefined, {
          weekday: "long",
          month: "long",
          day: "numeric",
        })}, ${events.length + reminders.length} items`}
        onclick={() => openDay(day)}
      >
        <span class="mobile-calendar-date" class:mobile-calendar-date-today={today}>
          {day.getDate()}
        </span>
        <span class="mobile-calendar-cell-items">
          {#each reminders.slice(0, reminderLimit) as reminder (reminder.id)}
            <span class="mobile-calendar-cell-item mobile-calendar-cell-reminder">
              <i aria-hidden="true"></i>
              <span>{reminder.title}</span>
            </span>
          {/each}
          {#each events.slice(0, eventLimit) as event (`${event.uid}:${event.recurrence_id ?? event.dtstart}`)}
            <span
              class="mobile-calendar-cell-item"
              class:mobile-calendar-cell-allday={event.all_day}
            >
              <i aria-hidden="true"></i>
              <span>{event.summary}</span>
            </span>
          {/each}
          {#if hiddenCount > 0}
            <span class="mobile-calendar-cell-more">+{hiddenCount}</span>
          {/if}
        </span>
      </button>
    {/each}
  </div>
</div>

<style>
  .mobile-calendar-month {
    min-height: 100%;
    padding: 0.15rem 0.75rem 5rem;
  }

  .mobile-calendar-weekdays,
  .mobile-calendar-month-grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
  }

  .mobile-calendar-weekdays span {
    padding: 0.45rem 0 0.5rem;
    text-align: center;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--shell-muted));
  }

  .mobile-calendar-month-grid {
    border-top: 1px solid rgb(var(--shell-border) / 0.58);
    touch-action: pan-y;
    outline: none;
  }

  .mobile-calendar-cell {
    display: flex;
    min-width: 0;
    min-height: 5rem;
    flex-direction: column;
    align-items: center;
    gap: 0.28rem;
    overflow: hidden;
    border-bottom: 1px solid rgb(var(--shell-border) / 0.5);
    padding: 0.38rem 0.12rem 0.32rem;
    text-align: center;
    color: rgb(var(--theme-text));
  }

  .mobile-calendar-cell:active,
  .mobile-calendar-cell-selected {
    background: rgb(var(--color-primary-500) / 0.07);
  }

  .mobile-calendar-cell-out {
    opacity: 0.38;
  }

  .mobile-calendar-date {
    display: inline-flex;
    min-width: 1.9rem;
    height: 1.9rem;
    align-items: center;
    justify-content: center;
    border-radius: 9999px;
    font-size: 1rem;
    font-weight: 600;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }

  .mobile-calendar-date-today {
    background: rgb(var(--color-primary-500));
    color: rgb(var(--on-primary));
    font-weight: 700;
  }

  .mobile-calendar-cell-items {
    display: flex;
    width: 100%;
    min-width: 0;
    flex-direction: column;
    gap: 0.12rem;
  }

  .mobile-calendar-cell-item {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.18rem;
    overflow: hidden;
    border-radius: 0.22rem;
    padding: 0.06rem 0.08rem;
    font-size: 0.625rem;
    line-height: 1.2;
    color: rgb(var(--theme-text-secondary));
  }

  .mobile-calendar-cell-item i {
    width: 0.15rem;
    height: 0.7rem;
    flex-shrink: 0;
    border-radius: 9999px;
    background: rgb(var(--color-primary-400));
  }

  .mobile-calendar-cell-item span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mobile-calendar-cell-allday {
    background: rgb(var(--color-primary-500) / 0.14);
    color: rgb(var(--color-primary-200));
  }

  .mobile-calendar-cell-reminder i {
    width: 0.42rem;
    height: 0.42rem;
    border: 1px solid rgb(var(--color-secondary-300) / 0.9);
    border-radius: 0.13rem;
    background: transparent;
  }

  .mobile-calendar-cell-more {
    align-self: flex-start;
    padding-left: 0.25rem;
    font-size: 0.59375rem;
    color: rgb(var(--shell-muted));
  }
</style>
