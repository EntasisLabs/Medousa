<script lang="ts">
  import { CalendarDays, Plus } from "@lucide/svelte";
  import { calendar, calendarDateUtils } from "$lib/stores/calendar.svelte";
  import type { CalendarEvent } from "$lib/types/calendar";
  import { onMount } from "svelte";

  interface Props {
    onPickEvent?: (event: CalendarEvent) => void;
    chrome?: "default" | "rail-list";
  }

  let { onPickEvent, chrome = "rail-list" }: Props = $props();

  const { addDays, startOfDay, isoDay } = calendarDateUtils;

  onMount(() => {
    void calendar.refresh();
  });

  type DayBucket = { day: Date; label: string; events: CalendarEvent[] };

  const buckets = $derived.by((): DayBucket[] => {
    const { from, to } = calendar.rangeForView();
    const out: DayBucket[] = [];
    let cursor = startOfDay(from);
    const end = startOfDay(to);
    // Cap list length for month grid (42 cells) — show days that have events.
    while (cursor < end) {
      const events = calendar.eventsForDay(cursor);
      if (events.length > 0) {
        out.push({
          day: cursor,
          label: cursor.toLocaleDateString(undefined, {
            weekday: "short",
            month: "short",
            day: "numeric",
          }),
          events,
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
    calendar.openCreate();
    onPickEvent?.({
      uid: "",
      summary: "",
      dtstart: calendar.selectedDay.toISOString(),
      all_day: false,
      calendar_path: calendar.calendarPath,
    });
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-chrome={chrome}>
  {#if calendar.error}
    <p class="px-3 py-2 text-xs text-warning-400">{calendar.error}</p>
  {/if}

  {#if buckets.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 px-3 py-6 text-center">
      <CalendarDays size={22} strokeWidth={1.5} class="text-surface-500" />
      <p class="text-sm text-surface-300">No events in this range</p>
      <button type="button" class="btn btn-sm btn-primary" onclick={create}>
        <Plus size={14} strokeWidth={2} />
        New event
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
                  <span class="min-w-0 flex-1">
                    <span class="block truncate text-[13px] font-medium text-surface-100">
                      {event.summary || "Untitled"}
                    </span>
                    {#if event.location}
                      <span class="block truncate text-[11px] text-surface-500">
                        {event.location}
                      </span>
                    {/if}
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
