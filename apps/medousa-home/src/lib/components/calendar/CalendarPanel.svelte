<script lang="ts">
  import { ChevronLeft, ChevronRight, Download, Plus, Upload } from "@lucide/svelte";
  import EventEditor from "$lib/components/calendar/EventEditor.svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { calendar, calendarDateUtils } from "$lib/stores/calendar.svelte";
  import type { CalendarEvent } from "$lib/types/calendar";
  import { onMount } from "svelte";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, mobile = false, embedded = false }: Props = $props();
  let importInput: HTMLInputElement | undefined = $state();
  let mobileDefaulted = $state(false);

  const { addDays, startOfWeek, startOfMonth, isoDay } = calendarDateUtils;

  onMount(() => {
    void calendar.refresh();
  });

  $effect(() => {
    if (visible) void calendar.refresh();
  });

  // Phones default to Day — month grid is too cramped for event titles.
  $effect(() => {
    if (!mobile || !visible || mobileDefaulted) return;
    mobileDefaulted = true;
    if (calendar.viewMode === "month") {
      calendar.setViewMode("day");
    }
  });

  $effect(() => {
    if (!mobile || !visible) return;
    return registerMobileBackHandler(() => {
      if (!calendar.editorOpen) return false;
      calendar.closeEditor();
      return true;
    });
  });

  const weekdayLabels = $derived(
    mobile
      ? ["M", "T", "W", "T", "F", "S", "S"]
      : ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
  );

  const monthCells = $derived.by(() => {
    const monthStart = startOfMonth(calendar.anchor);
    const gridStart = startOfWeek(monthStart);
    return Array.from({ length: 42 }, (_, index) => addDays(gridStart, index));
  });

  const weekDays = $derived.by(() => {
    const start = startOfWeek(calendar.selectedDay);
    return Array.from({ length: 7 }, (_, index) => addDays(start, index));
  });

  const dayEvents = $derived(calendar.eventsForDay(calendar.selectedDay));

  const monthTitle = $derived(
    calendar.anchor.toLocaleDateString(undefined, { month: "long", year: "numeric" }),
  );

  const rangeTitle = $derived.by(() => {
    if (calendar.viewMode === "week") {
      const start = startOfWeek(calendar.selectedDay);
      const end = addDays(start, 6);
      return `${start.toLocaleDateString(undefined, { month: "short", day: "numeric" })} – ${end.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`;
    }
    if (calendar.viewMode === "day") {
      return calendar.selectedDay.toLocaleDateString(undefined, {
        weekday: "long",
        month: "long",
        day: "numeric",
        year: "numeric",
      });
    }
    return monthTitle;
  });

  function isSameDay(a: Date, b: Date): boolean {
    return isoDay(a) === isoDay(b);
  }

  function isToday(date: Date): boolean {
    return isSameDay(date, new Date());
  }

  function formatTime(event: CalendarEvent): string {
    if (event.all_day) return "All day";
    return new Date(event.dtstart).toLocaleTimeString(undefined, {
      hour: "numeric",
      minute: "2-digit",
    });
  }

  function eventKey(event: CalendarEvent): string {
    return `${event.uid}:${event.recurrence_id ?? event.dtstart}`;
  }

  async function handleExport() {
    try {
      const ics = await calendar.exportIcs();
      const blob = new Blob([ics], { type: "text/calendar" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "personal.ics";
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      calendar.error = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleImportFile(file: File | undefined) {
    if (!file) return;
    try {
      const text = await file.text();
      await calendar.importIcs(text);
    } catch (err) {
      calendar.error = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<div
  class="calendar-surface relative flex h-full min-h-0 flex-col {visible ? '' : 'hidden'}"
  class:calendar-surface-mobile={mobile}
  class:calendar-surface-embedded={embedded}
>
  <header class="calendar-chrome">
    <div class="calendar-chrome-left">
      <h1 class="calendar-month-title">{rangeTitle}</h1>
    </div>

    <div class="calendar-chrome-center" role="tablist" aria-label="Calendar view">
      {#each ["day", "week", "month"] as mode (mode)}
        <button
          type="button"
          role="tab"
          class="calendar-seg"
          class:calendar-seg-active={calendar.viewMode === mode}
          aria-selected={calendar.viewMode === mode}
          onclick={() => calendar.setViewMode(mode as "month" | "week" | "day")}
        >
          {mode[0].toUpperCase() + mode.slice(1)}
        </button>
      {/each}
    </div>

    <div class="calendar-chrome-right">
      <div class="calendar-nav-group">
        <button
          type="button"
          class="calendar-icon-btn"
          aria-label="Previous"
          onclick={() => calendar.shift(-1)}
        >
          <ChevronLeft size={16} strokeWidth={1.75} />
        </button>
        <button type="button" class="calendar-today-btn" onclick={() => calendar.goToday()}
          >Today</button
        >
        <button
          type="button"
          class="calendar-icon-btn"
          aria-label="Next"
          onclick={() => calendar.shift(1)}
        >
          <ChevronRight size={16} strokeWidth={1.75} />
        </button>
      </div>

      <button
        type="button"
        class="calendar-icon-btn"
        onclick={() => importInput?.click()}
        title="Import .ics"
        aria-label="Import calendar"
      >
        <Upload size={15} strokeWidth={1.75} />
      </button>
      <button
        type="button"
        class="calendar-icon-btn"
        onclick={() => void handleExport()}
        title="Export .ics"
        aria-label="Export calendar"
      >
        <Download size={15} strokeWidth={1.75} />
      </button>
      <button
        type="button"
        class="calendar-icon-btn calendar-icon-btn-accent"
        onclick={() => calendar.openCreate(calendar.selectedDay)}
        aria-label="New event"
        title="New event"
      >
        <Plus size={16} strokeWidth={2} />
      </button>
    </div>

    <input
      bind:this={importInput}
      type="file"
      accept=".ics,text/calendar"
      class="hidden"
      onchange={(e) => {
        const file = e.currentTarget.files?.[0];
        void handleImportFile(file);
        e.currentTarget.value = "";
      }}
    />
  </header>

  {#if calendar.error}
    <p class="calendar-error">{calendar.error}</p>
  {/if}

  <div class="calendar-body">
    {#if calendar.loading && calendar.events.length === 0}
      <p class="calendar-loading">Loading…</p>
    {:else if calendar.viewMode === "month"}
      <div class="calendar-month">
        <div class="calendar-weekdays">
          {#each weekdayLabels as label}
            <div class="calendar-weekday">{label}</div>
          {/each}
        </div>
        <div class="calendar-month-grid">
          {#each monthCells as day (isoDay(day))}
            {@const inMonth = day.getMonth() === calendar.anchor.getMonth()}
            {@const selected = isSameDay(day, calendar.selectedDay)}
            {@const today = isToday(day)}
            {@const events = calendar.eventsForDay(day)}
            <div
              class="calendar-cell"
              class:calendar-cell-out={!inMonth}
              class:calendar-cell-selected={selected}
              role="button"
              tabindex="0"
              onclick={() => {
                const already = isSameDay(day, calendar.selectedDay);
                calendar.selectDay(day);
                if (mobile && already) calendar.openCreate(day);
              }}
              ondblclick={() => {
                if (!mobile) calendar.openCreate(day);
              }}
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  calendar.selectDay(day);
                }
              }}
            >
              <span class="calendar-cell-num" class:calendar-cell-num-today={today}>
                {day.getDate()}
              </span>
              {#if mobile}
                <div class="calendar-cell-dots" aria-hidden="true">
                  {#each events.slice(0, 3) as event (eventKey(event))}
                    <i class="calendar-cell-dot" class:calendar-cell-dot-allday={event.all_day}></i>
                  {/each}
                </div>
              {:else}
                <div class="calendar-cell-events">
                  {#each events.slice(0, 3) as event (eventKey(event))}
                    <button
                      type="button"
                      class="calendar-dot-event"
                      class:calendar-pill-event={event.all_day}
                      title={`${formatTime(event)} · ${event.summary}`}
                      onclick={(e) => {
                        e.stopPropagation();
                        calendar.openEdit(event);
                      }}
                    >
                      {#if !event.all_day}
                        <i class="calendar-dot-bar" aria-hidden="true"></i>
                      {/if}
                      <span class="calendar-dot-label">{event.summary}</span>
                    </button>
                  {/each}
                  {#if events.length > 3}
                    <span class="calendar-cell-more">{events.length - 3} more</span>
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </div>

      {#if mobile || dayEvents.length > 0}
        <section class="calendar-day-strip">
          <div class="calendar-day-strip-head">
            <h2 class="calendar-day-strip-title">
              {calendar.selectedDay.toLocaleDateString(undefined, {
                weekday: "long",
                month: "short",
                day: "numeric",
              })}
            </h2>
            {#if mobile}
              <button
                type="button"
                class="calendar-day-strip-add"
                onclick={() => calendar.openCreate(calendar.selectedDay)}
              >
                <Plus size={14} strokeWidth={2} />
                Add
              </button>
            {/if}
          </div>
          {#if dayEvents.length === 0}
            <p class="calendar-day-strip-empty">Nothing scheduled</p>
          {:else}
            <ul class="calendar-agenda">
              {#each dayEvents as event (eventKey(event))}
                <li>
                  <button
                    type="button"
                    class="calendar-agenda-row"
                    onclick={() => calendar.openEdit(event)}
                  >
                    <span class="calendar-agenda-time">{formatTime(event)}</span>
                    <i class="calendar-agenda-bar" aria-hidden="true"></i>
                    <span class="calendar-agenda-title">{event.summary}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/if}
    {:else if calendar.viewMode === "week"}
      <div class="calendar-week">
        {#each weekDays as day (isoDay(day))}
          {@const events = calendar.eventsForDay(day)}
          {@const today = isToday(day)}
          {@const selected = isSameDay(day, calendar.selectedDay)}
          <div
            class="calendar-week-col"
            class:calendar-week-col-selected={selected}
          >
            <button
              type="button"
              class="calendar-week-head"
              onclick={() => {
                const already = isSameDay(day, calendar.selectedDay);
                calendar.selectDay(day);
                if (mobile && already) calendar.openCreate(day);
              }}
              ondblclick={() => {
                if (!mobile) calendar.openCreate(day);
              }}
            >
              <span class="calendar-week-dow"
                >{day.toLocaleDateString(undefined, { weekday: "short" })}</span
              >
              <span class="calendar-week-num" class:calendar-cell-num-today={today}
                >{day.getDate()}</span
              >
            </button>
            <div class="calendar-week-body">
              {#each events as event (eventKey(event))}
                <button
                  type="button"
                  class="calendar-dot-event calendar-dot-event-block"
                  class:calendar-pill-event={event.all_day}
                  onclick={() => calendar.openEdit(event)}
                >
                  {#if !event.all_day}
                    <i class="calendar-dot-bar" aria-hidden="true"></i>
                  {/if}
                  <span class="calendar-dot-meta">{formatTime(event)}</span>
                  <span class="calendar-dot-label">{event.summary}</span>
                </button>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <div class="calendar-day-view">
        {#if dayEvents.length === 0}
          <div class="calendar-day-empty">
            <p class="calendar-day-empty-title">Nothing today</p>
            <p class="calendar-day-empty-copy">Tap + to add something.</p>
            <button
              type="button"
              class="calendar-ghost-add"
              onclick={() => calendar.openCreate(calendar.selectedDay)}
            >
              <Plus size={14} strokeWidth={2} />
              New event
            </button>
          </div>
        {:else}
          <ul class="calendar-agenda calendar-agenda-spacious">
            {#each dayEvents as event (eventKey(event))}
              <li>
                <button
                  type="button"
                  class="calendar-agenda-row"
                  onclick={() => calendar.openEdit(event)}
                >
                  <span class="calendar-agenda-time">{formatTime(event)}</span>
                  <i class="calendar-agenda-bar" aria-hidden="true"></i>
                  <div class="min-w-0 flex-1 text-left">
                    <div class="calendar-agenda-title">{event.summary}</div>
                    {#if event.location}
                      <div class="calendar-agenda-sub">{event.location}</div>
                    {/if}
                  </div>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}
  </div>

  {#if calendar.editorOpen}
    <EventEditor
      event={calendar.editing}
      defaultDay={calendar.selectedDay}
      {mobile}
      onClose={() => calendar.closeEditor()}
      onSave={async (payload) => {
        await calendar.saveEvent({
          summary: payload.summary,
          description: payload.description || null,
          location: payload.location || null,
          dtstart: payload.dtstart,
          dtend: payload.dtend,
          all_day: payload.all_day,
        });
      }}
      onDelete={calendar.editing
        ? async () => {
            if (calendar.editing) await calendar.removeEvent(calendar.editing.uid);
          }
        : undefined}
    />
  {/if}
</div>

<style>
  .calendar-surface {
    background: transparent;
  }

  .calendar-chrome {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem 1rem;
    padding: 0.85rem 1.1rem 0.7rem;
    border-bottom: 1px solid rgb(var(--shell-border) / var(--shell-border-soft));
  }

  .calendar-chrome-left {
    min-width: 0;
    flex: 1 1 10rem;
  }

  .calendar-month-title {
    margin: 0;
    font-size: 1.55rem;
    font-weight: 620;
    letter-spacing: -0.03em;
    line-height: 1.15;
    color: rgb(var(--color-surface-50));
  }

  .calendar-chrome-center {
    display: inline-flex;
    align-items: center;
    gap: 0.15rem;
    padding: 0.18rem;
    border-radius: 0.55rem;
    background: rgb(var(--color-surface-800) / 0.55);
    border: 1px solid rgb(var(--shell-border) / 0.55);
  }

  .calendar-seg {
    min-height: 1.55rem;
    border-radius: 0.4rem;
    padding: 0.2rem 0.7rem;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--shell-muted));
    transition:
      background 140ms ease,
      color 140ms ease;
  }

  .calendar-seg:hover {
    color: rgb(var(--color-surface-200));
  }

  .calendar-seg-active {
    background: rgb(var(--color-surface-700) / 0.85);
    color: rgb(var(--color-surface-50));
    box-shadow: 0 1px 0 rgb(255 255 255 / 0.04) inset;
  }

  .calendar-chrome-right {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    margin-left: auto;
  }

  .calendar-nav-group {
    display: inline-flex;
    align-items: center;
    gap: 0.1rem;
    margin-right: 0.35rem;
  }

  .calendar-icon-btn {
    display: inline-flex;
    height: 1.85rem;
    width: 1.85rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.45rem;
    color: rgb(var(--shell-icon));
    transition:
      background 140ms ease,
      color 140ms ease;
  }

  .calendar-icon-btn:hover {
    background: rgb(var(--color-surface-700) / 0.55);
    color: rgb(var(--shell-icon-hover));
  }

  .calendar-icon-btn-accent {
    color: rgb(var(--color-primary-300));
  }

  .calendar-icon-btn-accent:hover {
    background: rgb(var(--color-primary-500) / 0.16);
    color: rgb(var(--color-primary-200));
  }

  .calendar-today-btn {
    min-height: 1.85rem;
    border-radius: 0.45rem;
    padding: 0 0.55rem;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--shell-label));
    transition:
      background 140ms ease,
      color 140ms ease;
  }

  .calendar-today-btn:hover {
    background: rgb(var(--color-surface-700) / 0.55);
    color: rgb(var(--color-surface-100));
  }

  .calendar-error {
    margin: 0;
    padding: 0.45rem 1.1rem;
    font-size: 0.75rem;
    color: rgb(var(--color-error-400));
  }

  .calendar-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0.35rem 0.85rem 1rem;
  }

  .calendar-loading {
    margin: 2rem 0;
    text-align: center;
    font-size: 0.8125rem;
    color: rgb(var(--shell-muted));
  }

  .calendar-weekdays,
  .calendar-month-grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
  }

  .calendar-weekday {
    padding: 0.45rem 0.35rem 0.35rem;
    text-align: center;
    font-size: 0.6875rem;
    font-weight: 550;
    color: rgb(var(--shell-muted));
  }

  .calendar-month-grid {
    border-top: 1px solid rgb(var(--shell-border) / 0.55);
    border-left: 1px solid rgb(var(--shell-border) / 0.55);
  }

  .calendar-cell {
    position: relative;
    min-height: 6.25rem;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 0.2rem;
    padding: 0.35rem 0.35rem 0.4rem;
    text-align: left;
    border-right: 1px solid rgb(var(--shell-border) / 0.55);
    border-bottom: 1px solid rgb(var(--shell-border) / 0.55);
    background: transparent;
    transition: background 140ms ease;
  }

  .calendar-cell:hover {
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .calendar-cell-out {
    color: rgb(var(--shell-muted));
  }

  .calendar-cell-out .calendar-cell-num {
    opacity: 0.38;
  }

  .calendar-cell-selected {
    background: rgb(var(--color-primary-500) / 0.07);
  }

  .calendar-cell-num {
    align-self: flex-end;
    display: inline-flex;
    height: 1.45rem;
    min-width: 1.45rem;
    align-items: center;
    justify-content: center;
    border-radius: 9999px;
    padding: 0 0.2rem;
    font-size: 0.75rem;
    font-weight: 560;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-surface-200));
    line-height: 1;
  }

  .calendar-cell-num-today {
    background: rgb(var(--color-primary-500));
    color: rgb(var(--color-surface-50));
    font-weight: 650;
  }

  .calendar-cell-events {
    display: flex;
    flex-direction: column;
    gap: 0.12rem;
    min-height: 0;
    margin-top: 0.1rem;
  }

  .calendar-dot-event {
    display: flex;
    align-items: center;
    gap: 0.28rem;
    min-width: 0;
    border-radius: 0.28rem;
    padding: 0.08rem 0.2rem;
    text-align: left;
    transition: background 120ms ease;
  }

  .calendar-dot-event:hover {
    background: rgb(var(--color-surface-700) / 0.45);
  }

  .calendar-dot-event-block {
    width: 100%;
    flex-wrap: wrap;
    padding: 0.28rem 0.35rem;
    gap: 0.15rem 0.35rem;
  }

  .calendar-pill-event {
    background: rgb(var(--color-primary-500) / 0.22);
    color: rgb(var(--color-primary-100));
  }

  .calendar-pill-event:hover {
    background: rgb(var(--color-primary-500) / 0.3);
  }

  .calendar-dot-bar,
  .calendar-agenda-bar {
    width: 0.18rem;
    height: 0.7rem;
    flex-shrink: 0;
    border-radius: 9999px;
    background: rgb(var(--color-primary-400));
  }

  .calendar-dot-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.6875rem;
    line-height: 1.2;
    color: rgb(var(--color-surface-100));
  }

  .calendar-pill-event .calendar-dot-label {
    color: rgb(var(--color-primary-100));
  }

  .calendar-dot-meta {
    font-size: 0.625rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--shell-muted));
  }

  .calendar-cell-more {
    padding-left: 0.2rem;
    font-size: 0.625rem;
    color: rgb(var(--shell-muted));
  }

  .calendar-day-strip {
    margin-top: 0.85rem;
    padding: 0.15rem 0.15rem 0;
  }

  .calendar-day-strip-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.45rem;
  }

  .calendar-day-strip-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-200));
  }

  .calendar-day-strip-add {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    min-height: 1.65rem;
    border-radius: 0.45rem;
    padding: 0 0.55rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-primary-200));
    background: rgb(var(--color-primary-500) / 0.12);
  }

  .calendar-day-strip-empty {
    margin: 0;
    padding: 0.35rem 0.15rem 0.55rem;
    font-size: 0.75rem;
    color: rgb(var(--shell-muted));
  }

  .calendar-cell-dots {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.18rem;
    margin-top: auto;
    padding-bottom: 0.1rem;
  }

  .calendar-cell-dot {
    width: 0.28rem;
    height: 0.28rem;
    border-radius: 9999px;
    background: rgb(var(--color-primary-400));
  }

  .calendar-cell-dot-allday {
    background: rgb(var(--color-primary-300) / 0.75);
  }

  .calendar-agenda {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .calendar-agenda-spacious {
    max-width: 34rem;
    gap: 0.2rem;
  }

  .calendar-agenda-row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.7rem;
    border-radius: 0.55rem;
    padding: 0.55rem 0.65rem;
    transition: background 140ms ease;
  }

  .calendar-agenda-row:hover {
    background: rgb(var(--color-surface-800) / 0.55);
  }

  .calendar-agenda-time {
    width: 4.1rem;
    flex-shrink: 0;
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--shell-muted));
  }

  .calendar-agenda-title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.875rem;
    color: rgb(var(--color-surface-50));
  }

  .calendar-agenda-sub {
    margin-top: 0.1rem;
    font-size: 0.6875rem;
    color: rgb(var(--shell-muted));
  }

  .calendar-week {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 0;
    min-height: 20rem;
    border-top: 1px solid rgb(var(--shell-border) / 0.55);
    border-left: 1px solid rgb(var(--shell-border) / 0.55);
  }

  .calendar-week-col {
    display: flex;
    min-height: 0;
    flex-direction: column;
    border-right: 1px solid rgb(var(--shell-border) / 0.55);
    border-bottom: 1px solid rgb(var(--shell-border) / 0.55);
  }

  .calendar-week-col-selected {
    background: rgb(var(--color-primary-500) / 0.05);
  }

  .calendar-week-head {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    padding: 0.65rem 0.4rem 0.5rem;
  }

  .calendar-week-dow {
    font-size: 0.6875rem;
    font-weight: 550;
    color: rgb(var(--shell-muted));
  }

  .calendar-week-num {
    display: inline-flex;
    height: 1.55rem;
    min-width: 1.55rem;
    align-items: center;
    justify-content: center;
    border-radius: 9999px;
    font-size: 0.8125rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-surface-100));
  }

  .calendar-week-body {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.2rem 0.35rem 0.5rem;
    overflow: auto;
  }

  .calendar-day-view {
    padding: 0.5rem 0.25rem 1rem;
  }

  .calendar-day-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    min-height: 14rem;
    text-align: center;
  }

  .calendar-day-empty-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: rgb(var(--color-surface-200));
  }

  .calendar-day-empty-copy {
    margin: 0;
    font-size: 0.8125rem;
    color: rgb(var(--shell-muted));
  }

  .calendar-ghost-add {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.55rem;
    border-radius: 0.55rem;
    border: 1px solid rgb(var(--shell-border));
    background: rgb(var(--color-surface-800) / 0.4);
    padding: 0.4rem 0.75rem;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--color-surface-200));
    transition:
      background 140ms ease,
      border-color 140ms ease;
  }

  .calendar-ghost-add:hover {
    border-color: rgb(var(--color-primary-500) / 0.4);
    background: rgb(var(--color-primary-500) / 0.1);
  }

  @media (max-width: 900px) {
    .calendar-chrome {
      padding: 0.75rem 0.85rem;
    }

    .calendar-month-title {
      font-size: 1.2rem;
    }

    .calendar-week {
      grid-template-columns: 1fr;
    }

    .calendar-cell {
      min-height: 4rem;
    }
  }

  .calendar-surface-mobile .calendar-chrome {
    gap: 0.55rem 0.65rem;
    padding: 0.65rem 0.75rem 0.55rem;
  }

  .calendar-surface-mobile .calendar-month-title {
    font-size: 1.05rem;
  }

  .calendar-surface-mobile .calendar-body {
    padding: 0.25rem 0.65rem 0.85rem;
    overflow: auto;
  }

  .calendar-surface-mobile .calendar-cell {
    min-height: 2.65rem;
    align-items: center;
    padding: 0.25rem 0.1rem 0.3rem;
  }

  .calendar-surface-mobile .calendar-cell-num {
    align-self: center;
  }

  .calendar-surface-mobile .calendar-weekday {
    padding: 0.3rem 0 0.2rem;
    font-size: 0.625rem;
  }

  .calendar-surface-mobile .calendar-day-strip {
    margin-top: 0.65rem;
    border-top: 1px solid rgb(var(--shell-border) / 0.55);
    padding-top: 0.65rem;
  }

  .calendar-surface-mobile .calendar-week-col {
    min-height: auto;
  }
</style>
