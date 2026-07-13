<script lang="ts">
  import { MapPin, X } from "@lucide/svelte";
  import type { CalendarEvent } from "$lib/types/calendar";
  import { calendarDateUtils } from "$lib/stores/calendar.svelte";

  interface Props {
    event: CalendarEvent | null;
    defaultDay: Date;
    onClose: () => void;
    onSave: (payload: {
      summary: string;
      description: string;
      location: string;
      dtstart: string;
      dtend: string;
      all_day: boolean;
    }) => Promise<void>;
    onDelete?: () => Promise<void>;
  }

  let { event, defaultDay, onClose, onSave, onDelete }: Props = $props();

  const { isoDay, allDayKey, nextAllDayKey, allDayBoundIso } = calendarDateUtils;

  function toDateInput(value: Date): string {
    return isoDay(value);
  }

  function toTimeInput(value: Date): string {
    return `${String(value.getHours()).padStart(2, "0")}:${String(value.getMinutes()).padStart(2, "0")}`;
  }

  function defaultCreateStart(day: Date): Date {
    const now = new Date();
    const start = new Date(day);
    if (
      start.getFullYear() === now.getFullYear() &&
      start.getMonth() === now.getMonth() &&
      start.getDate() === now.getDate()
    ) {
      start.setHours(now.getHours() + 1, 0, 0, 0);
    } else {
      start.setHours(9, 0, 0, 0);
    }
    return start;
  }

  const initialStart = event
    ? event.all_day
      ? new Date(`${allDayKey(event.dtstart)}T12:00:00`)
      : new Date(event.dtstart)
    : defaultCreateStart(defaultDay);
  const initialEnd = event?.dtend
    ? event.all_day
      ? new Date(`${allDayKey(event.dtend)}T12:00:00`)
      : new Date(event.dtend)
    : new Date(initialStart.getTime() + 60 * 60 * 1000);

  let summary = $state(event?.summary ?? "");
  let description = $state(event?.description ?? "");
  let location = $state(event?.location ?? "");
  let allDay = $state(event?.all_day ?? false);
  let date = $state(
    event?.all_day ? allDayKey(event.dtstart) : toDateInput(initialStart),
  );
  let startTime = $state(toTimeInput(initialStart));
  let endTime = $state(
    event?.all_day
      ? toTimeInput(initialStart)
      : toTimeInput(initialEnd),
  );
  let saving = $state(false);
  let error = $state<string | null>(null);
  let titleEl: HTMLInputElement | undefined = $state();

  $effect(() => {
    queueMicrotask(() => titleEl?.focus());
  });

  const whenSummary = $derived.by(() => {
    const day = new Date(`${date}T12:00:00`);
    const dayLabel = day.toLocaleDateString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
      year: "numeric",
    });
    if (allDay) return `${dayLabel} · All day`;
    const start = new Date(`${date}T${startTime}:00`);
    const end = new Date(`${date}T${endTime}:00`);
    const fmt = (value: Date) =>
      value.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
    return `${dayLabel}  ${fmt(start)} – ${fmt(end)}`;
  });

  async function submit() {
    const title = summary.trim() || "New Event";
    saving = true;
    error = null;
    try {
      let dtstart: string;
      let dtend: string;
      if (allDay) {
        // DATE values: store UTC midnight of the chosen calendar day (not local midnight).
        dtstart = allDayBoundIso(date);
        dtend = allDayBoundIso(nextAllDayKey(date));
      } else {
        // Timed events: interpret in local timezone, persist as UTC.
        dtstart = new Date(`${date}T${startTime}:00`).toISOString();
        dtend = new Date(`${date}T${endTime}:00`).toISOString();
      }
      await onSave({
        summary: title,
        description: description.trim(),
        location: location.trim(),
        dtstart,
        dtend,
        all_day: allDay,
      });
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      void submit();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="cal-pop-backdrop"
  role="presentation"
  onclick={onClose}
  onkeydown={onKeydown}
>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="cal-pop"
    role="dialog"
    aria-modal="true"
    aria-label={event ? "Edit event" : "New event"}
    onclick={(e) => e.stopPropagation()}
    onkeydown={onKeydown}
  >
    <div class="cal-pop-grab" aria-hidden="true"></div>

    <header class="cal-pop-head">
      <div class="cal-pop-mode">
        <span class="cal-pop-mode-active">{event ? "Event" : "New Event"}</span>
      </div>
      <button type="button" class="cal-pop-x" aria-label="Close" onclick={onClose}>
        <X size={15} strokeWidth={1.75} />
      </button>
    </header>

    <div class="cal-pop-card cal-pop-card-title">
      <input
        bind:this={titleEl}
        class="cal-pop-title"
        bind:value={summary}
        placeholder="New Event"
        maxlength={200}
      />
      <button
        type="button"
        class="cal-pop-switch"
        class:cal-pop-switch-on={allDay}
        aria-pressed={allDay}
        title="All day"
        onclick={() => (allDay = !allDay)}
      >
        <span class="cal-pop-switch-knob"></span>
      </button>
    </div>

    <div class="cal-pop-card">
      <div class="cal-pop-when-summary">{whenSummary}</div>
      <div class="cal-pop-when-edit">
        <input class="cal-pop-field" type="date" bind:value={date} aria-label="Date" />
        {#if !allDay}
          <input class="cal-pop-field" type="time" bind:value={startTime} aria-label="Starts" />
          <span class="cal-pop-dash">–</span>
          <input class="cal-pop-field" type="time" bind:value={endTime} aria-label="Ends" />
        {/if}
      </div>
      <p class="cal-pop-hint">{allDay ? "All-day event" : "Toggle the switch for all day"}</p>
    </div>

    <div class="cal-pop-card cal-pop-row">
      <MapPin size={14} strokeWidth={1.75} class="cal-pop-row-icon" />
      <input
        class="cal-pop-inline"
        bind:value={location}
        placeholder="Add Location"
      />
    </div>

    <div class="cal-pop-card">
      <textarea
        class="cal-pop-notes"
        rows="3"
        bind:value={description}
        placeholder="Add Notes"
      ></textarea>
    </div>

    {#if error}
      <p class="cal-pop-error">{error}</p>
    {/if}

    <footer class="cal-pop-foot">
      {#if event && onDelete}
        <button
          type="button"
          class="cal-pop-text-danger"
          disabled={saving}
          onclick={() => void onDelete()}
        >
          Delete
        </button>
      {:else}
        <span></span>
      {/if}
      <div class="cal-pop-foot-right">
        <button type="button" class="cal-pop-text" onclick={onClose}>Cancel</button>
        <button
          type="button"
          class="cal-pop-save"
          disabled={saving}
          onclick={() => void submit()}
        >
          {saving ? "Saving…" : "Add"}
        </button>
      </div>
    </footer>
  </div>
</div>

<style>
  .cal-pop-backdrop {
    position: absolute;
    inset: 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    background: rgb(var(--color-surface-950) / 0.28);
    backdrop-filter: blur(10px) saturate(1.15);
  }

  .cal-pop {
    width: min(22.5rem, 100%);
    border-radius: 0.95rem;
    border: 1px solid rgb(255 255 255 / 0.08);
    background: color-mix(in srgb, rgb(var(--shell-pane-bg)) 78%, transparent);
    box-shadow:
      0 1px 0 rgb(255 255 255 / 0.06) inset,
      0 24px 64px rgb(0 0 0 / 0.45),
      0 2px 8px rgb(0 0 0 / 0.25);
    backdrop-filter: blur(28px) saturate(1.35);
    padding: 0.55rem 0.7rem 0.7rem;
    animation: cal-pop-in 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  @keyframes cal-pop-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .cal-pop-grab {
    width: 2.25rem;
    height: 0.22rem;
    margin: 0.1rem auto 0.45rem;
    border-radius: 9999px;
    background: rgb(var(--shell-border));
    opacity: 0.7;
  }

  .cal-pop-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.55rem;
  }

  .cal-pop-mode {
    display: inline-flex;
    padding: 0.12rem;
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-900) / 0.45);
  }

  .cal-pop-mode-active {
    display: inline-flex;
    min-height: 1.45rem;
    align-items: center;
    border-radius: 0.4rem;
    background: rgb(var(--color-primary-500) / 0.88);
    padding: 0.15rem 0.65rem;
    font-size: 0.6875rem;
    font-weight: 650;
    letter-spacing: 0.01em;
    color: rgb(var(--color-surface-50));
  }

  .cal-pop-x {
    display: inline-flex;
    height: 1.6rem;
    width: 1.6rem;
    align-items: center;
    justify-content: center;
    border-radius: 9999px;
    color: rgb(var(--shell-icon));
    background: rgb(var(--color-surface-800) / 0.55);
  }

  .cal-pop-x:hover {
    color: rgb(var(--color-surface-100));
    background: rgb(var(--color-surface-700) / 0.7);
  }

  .cal-pop-card {
    border-radius: 0.7rem;
    border: 1px solid rgb(var(--shell-border) / 0.55);
    background: rgb(var(--color-surface-900) / 0.42);
    padding: 0.55rem 0.7rem;
    margin-bottom: 0.45rem;
  }

  .cal-pop-card-title {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    padding-top: 0.45rem;
    padding-bottom: 0.45rem;
  }

  .cal-pop-title {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    padding: 0.15rem 0;
    font-size: 1.05rem;
    font-weight: 560;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
    outline: none;
    box-shadow: none;
  }

  .cal-pop-title::placeholder {
    color: rgb(var(--shell-muted));
    font-weight: 450;
  }

  .cal-pop-title:focus {
    outline: none;
    box-shadow: none;
  }

  .cal-pop-switch {
    position: relative;
    width: 2.35rem;
    height: 1.3rem;
    flex-shrink: 0;
    border-radius: 9999px;
    background: rgb(var(--color-surface-700) / 0.85);
    transition: background 160ms ease;
  }

  .cal-pop-switch-on {
    background: rgb(var(--color-primary-500));
  }

  .cal-pop-switch-knob {
    position: absolute;
    top: 0.14rem;
    left: 0.14rem;
    width: 1.02rem;
    height: 1.02rem;
    border-radius: 9999px;
    background: rgb(var(--color-surface-50));
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.35);
    transition: transform 160ms ease;
  }

  .cal-pop-switch-on .cal-pop-switch-knob {
    transform: translateX(1.05rem);
  }

  .cal-pop-when-summary {
    font-size: 0.8125rem;
    font-weight: 550;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-100));
  }

  .cal-pop-when-edit {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.5rem;
  }

  .cal-pop-field {
    min-height: 1.7rem;
    border: 0;
    border-radius: 0.4rem;
    background: rgb(var(--color-surface-800) / 0.55);
    padding: 0.2rem 0.45rem;
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-surface-100));
    outline: none;
  }

  .cal-pop-field:focus {
    background: rgb(var(--color-surface-700) / 0.65);
  }

  .cal-pop-dash {
    color: rgb(var(--shell-muted));
    font-size: 0.75rem;
  }

  .cal-pop-hint {
    margin: 0.4rem 0 0;
    font-size: 0.6875rem;
    color: rgb(var(--shell-muted));
  }

  .cal-pop-row {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding-top: 0.5rem;
    padding-bottom: 0.5rem;
  }

  .cal-pop-row-icon {
    flex-shrink: 0;
    color: rgb(var(--shell-muted));
  }

  .cal-pop-inline {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.875rem;
    color: rgb(var(--color-surface-100));
    outline: none;
  }

  .cal-pop-inline::placeholder {
    color: rgb(var(--shell-muted));
  }

  .cal-pop-notes {
    width: 100%;
    min-height: 4.25rem;
    resize: none;
    border: 0;
    background: transparent;
    padding: 0.1rem 0;
    font-size: 0.875rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-100));
    outline: none;
  }

  .cal-pop-notes::placeholder {
    color: rgb(var(--shell-muted));
  }

  .cal-pop-error {
    margin: 0 0.2rem 0.45rem;
    font-size: 0.75rem;
    color: rgb(var(--color-error-400));
  }

  .cal-pop-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.35rem 0.15rem 0.1rem;
  }

  .cal-pop-foot-right {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }

  .cal-pop-text,
  .cal-pop-text-danger {
    border: 0;
    background: transparent;
    font-size: 0.8125rem;
    font-weight: 550;
    color: rgb(var(--shell-label));
  }

  .cal-pop-text:hover {
    color: rgb(var(--color-surface-100));
  }

  .cal-pop-text-danger {
    color: rgb(var(--color-error-400));
  }

  .cal-pop-save {
    min-height: 1.7rem;
    border: 0;
    border-radius: 0.5rem;
    background: rgb(var(--color-primary-500));
    padding: 0.3rem 0.85rem;
    font-size: 0.8125rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
    box-shadow: 0 1px 0 rgb(255 255 255 / 0.08) inset;
    transition:
      background 140ms ease,
      transform 100ms ease;
  }

  .cal-pop-save:hover:not(:disabled) {
    background: rgb(var(--color-primary-400));
  }

  .cal-pop-save:active:not(:disabled) {
    transform: scale(0.98);
  }

  .cal-pop-save:disabled {
    opacity: 0.55;
  }
</style>
