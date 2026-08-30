<script lang="ts">
  import { Bell, Check, FileText, MapPin, Plus, Repeat, X } from "@lucide/svelte";
  import type { CalendarAlarm, CalendarEvent } from "$lib/types/calendar";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { calendarDateUtils } from "$lib/stores/calendar.svelte";
  import { createVaultNote, getVaultNote } from "$lib/daemon";
  import {
    meetingNoteContent,
    meetingNotePath,
  } from "$lib/utils/calendarReminders";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    event: CalendarEvent | null;
    defaultDay: Date;
    mobile?: boolean;
    onClose: () => void;
    onSwitchKind?: (kind: "event" | "reminder") => void;
    onSave: (payload: {
      summary: string;
      description: string;
      location: string;
      dtstart: string;
      dtend: string;
      all_day: boolean;
      rrule: string | null;
      note_path: string | null;
      alarms: CalendarAlarm[];
    }) => Promise<void>;
    onDelete?: () => Promise<void>;
  }

  let {
    event,
    defaultDay,
    mobile = false,
    onClose,
    onSwitchKind,
    onSave,
    onDelete,
  }: Props = $props();

  const { isoDay, allDayKey, nextAllDayKey, allDayBoundIso } = calendarDateUtils;

  const ALARM_PRESETS: { label: string; minutes: number }[] = [
    { label: "At time of event", minutes: 0 },
    { label: "5 minutes before", minutes: 5 },
    { label: "15 minutes before", minutes: 15 },
    { label: "30 minutes before", minutes: 30 },
    { label: "1 hour before", minutes: 60 },
    { label: "1 day before", minutes: 1440 },
    { label: "2 days before", minutes: 2880 },
    { label: "1 week before", minutes: 10080 },
  ];

  const RRULE_PRESETS: { label: string; value: string | null }[] = [
    { label: "Does not repeat", value: null },
    { label: "Every day", value: "FREQ=DAILY" },
    { label: "Every week", value: "FREQ=WEEKLY" },
    { label: "Every month", value: "FREQ=MONTHLY" },
    { label: "Every year", value: "FREQ=YEARLY" },
  ];

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

  function normalizeRrule(raw: string | null | undefined): string | null {
    if (!raw?.trim()) return null;
    return raw.replace(/^RRULE:/i, "").trim() || null;
  }

  let summary = $state("");
  let description = $state("");
  let location = $state("");
  let allDay = $state(false);
  let date = $state("");
  let startTime = $state("");
  let endTime = $state("");
  let rrule = $state<string | null>(null);
  let notePath = $state("");
  let alarms = $state<CalendarAlarm[]>([]);
  let loadedSeed = "";
  let saving = $state(false);
  let linking = $state(false);
  let error = $state<string | null>(null);
  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  $effect.pre(() => {
    const source = event;
    const seed = source?.uid ?? `new:${isoDay(defaultDay)}`;
    if (seed === loadedSeed) return;
    loadedSeed = seed;
    const initialStart = source
      ? source.all_day
        ? new Date(`${allDayKey(source.dtstart)}T12:00:00`)
        : new Date(source.dtstart)
      : defaultCreateStart(defaultDay);
    const initialEnd = source?.dtend
      ? source.all_day
        ? new Date(`${allDayKey(source.dtend)}T12:00:00`)
        : new Date(source.dtend)
      : new Date(initialStart.getTime() + 60 * 60 * 1000);
    summary = source?.summary ?? "";
    description = source?.description ?? "";
    location = source?.location ?? "";
    allDay = source?.all_day ?? false;
    date = source?.all_day ? allDayKey(source.dtstart) : toDateInput(initialStart);
    startTime = toTimeInput(initialStart);
    endTime = source?.all_day ? toTimeInput(initialStart) : toTimeInput(initialEnd);
    rrule = normalizeRrule(source?.rrule);
    notePath = source?.note_path ?? "";
    alarms = (source?.alarms ?? []).map((alarm) => ({
      trigger_minutes_before: alarm.trigger_minutes_before,
      action: alarm.action ?? "display",
    }));
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

  const rruleLabel = $derived(
    RRULE_PRESETS.find((row) => row.value === rrule)?.label ??
      (rrule ? `Custom · ${rrule}` : "Does not repeat"),
  );

  function alarmLabel(minutes: number): string {
    return (
      ALARM_PRESETS.find((row) => row.minutes === minutes)?.label ??
      `${minutes} minutes before`
    );
  }

  function addAlarm(minutes: number) {
    if (minutes <= 0) return;
    if (alarms.some((alarm) => alarm.trigger_minutes_before === minutes)) return;
    alarms = [
      ...alarms,
      { trigger_minutes_before: minutes, action: "display" },
    ].sort((a, b) => a.trigger_minutes_before - b.trigger_minutes_before);
  }

  function removeAlarm(minutes: number) {
    alarms = alarms.filter((alarm) => alarm.trigger_minutes_before !== minutes);
  }

  async function openLinkedNote() {
    const path = notePath.trim();
    if (!path) return;
    linking = true;
    error = null;
    try {
      layout.navigateDesktop("library");
      await lmeWorkspace.openNote(path);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      linking = false;
    }
  }

  async function createLinkedNote() {
    linking = true;
    error = null;
    try {
      const title = summary.trim() || "Meeting";
      const path = meetingNotePath(title, date);
      try {
        await getVaultNote(path);
        notePath = path;
      } catch {
        await createVaultNote(
          path,
          meetingNoteContent(title, date, event?.uid),
        );
        notePath = path;
      }
      layout.navigateDesktop("library");
      await lmeWorkspace.openNote(path);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      linking = false;
    }
  }

  async function submit() {
    const title = summary.trim() || "New Event";
    saving = true;
    error = null;
    try {
      let dtstart: string;
      let dtend: string;
      if (allDay) {
        dtstart = allDayBoundIso(date);
        dtend = allDayBoundIso(nextAllDayKey(date));
      } else {
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
        rrule,
        note_path: notePath.trim() || null,
        alarms: alarms.filter((alarm) => alarm.trigger_minutes_before > 0),
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

  $effect(() => {
    if (!mobile) return;
    return registerMobileBackHandler(() => {
      onClose();
      return true;
    });
  });

  $effect(() => {
    if (!mobile || !sheetEl || !headerEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, {
      onDismiss: onClose,
    });
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="cal-pop-backdrop"
  class:cal-pop-backdrop-mobile={mobile}
  class:mobile-sheet-backdrop={mobile}
  role="presentation"
  onclick={onClose}
  onkeydown={onKeydown}
>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={sheetEl}
    class="cal-pop"
    class:cal-pop-mobile={mobile}
    class:mobile-sheet={mobile}
    class:calendar-editor-sheet={mobile}
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-label={event ? "Edit event" : "New event"}
    onclick={(e) => e.stopPropagation()}
    onkeydown={onKeydown}
  >
    {#if mobile}
      <div bind:this={headerEl} class="cal-pop-mobile-sheet-head">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <header class="cal-pop-mobile-head">
          <button type="button" class="cal-pop-x" aria-label="Close" onclick={onClose}>
            <X size={21} strokeWidth={1.75} />
          </button>
          <h2>{event ? "Edit Event" : "New"}</h2>
          <button
            type="button"
            class="cal-pop-mobile-save"
            aria-label={event ? "Save event" : "Add event"}
            disabled={saving}
            onclick={() => void submit()}
          >
            <Check size={22} strokeWidth={2} />
          </button>
        </header>
        {#if !event}
          <div class="cal-pop-kind" role="tablist" aria-label="Create type">
            <button type="button" role="tab" aria-selected="true" class="cal-pop-kind-active">
              Event
            </button>
            <button
              type="button"
              role="tab"
              aria-selected="false"
              onclick={() => onSwitchKind?.("reminder")}
            >
              Reminder
            </button>
          </div>
        {/if}
      </div>
    {:else}
      <div class="cal-pop-grab" aria-hidden="true"></div>
      <header class="cal-pop-head">
        <div class="cal-pop-mode">
          <span class="cal-pop-mode-active">{event ? "Event" : "New Event"}</span>
        </div>
        <button type="button" class="cal-pop-x" aria-label="Close" onclick={onClose}>
          <X size={15} strokeWidth={1.75} />
        </button>
      </header>
    {/if}

    <div class="cal-pop-scroll">
      <div class="cal-pop-card cal-pop-card-title">
        <input
          class="cal-pop-title"
          bind:value={summary}
          placeholder="New Event"
          maxlength={200}
        />
        {#if mobile}
          <input
            class="cal-pop-mobile-location"
            bind:value={location}
            placeholder="Location or video call"
          />
        {:else}
          <div class="cal-pop-all-day">
            <span class="cal-pop-switch-label">All day</span>
            <button
              type="button"
              class="cal-pop-switch"
              class:cal-pop-switch-on={allDay}
              aria-label="All day"
              aria-pressed={allDay}
              onclick={() => (allDay = !allDay)}
            >
              <span class="cal-pop-switch-knob"></span>
            </button>
          </div>
        {/if}
      </div>

      <div class="cal-pop-card">
        {#if mobile}
          <div class="cal-pop-mobile-field-row">
            <span>All-day</span>
            <button
              type="button"
              class="cal-pop-switch"
              class:cal-pop-switch-on={allDay}
              aria-label="All day"
              aria-pressed={allDay}
              onclick={() => (allDay = !allDay)}
            >
              <span class="cal-pop-switch-knob"></span>
            </button>
          </div>
          <label class="cal-pop-mobile-field-row">
            <span>Date</span>
            <input class="cal-pop-field" type="date" bind:value={date} />
          </label>
          {#if !allDay}
            <label class="cal-pop-mobile-field-row">
              <span>Starts</span>
              <input class="cal-pop-field" type="time" bind:value={startTime} />
            </label>
            <label class="cal-pop-mobile-field-row">
              <span>Ends</span>
              <input class="cal-pop-field" type="time" bind:value={endTime} />
            </label>
          {/if}
        {:else}
          <div class="cal-pop-when-summary">{whenSummary}</div>
          <div class="cal-pop-when-edit">
            <label class="cal-pop-field-group cal-pop-field-group-date">
              <span>Date</span>
              <input class="cal-pop-field" type="date" bind:value={date} />
            </label>
            {#if !allDay}
              <label class="cal-pop-field-group">
                <span>Starts</span>
                <input class="cal-pop-field" type="time" bind:value={startTime} />
              </label>
              <label class="cal-pop-field-group">
                <span>Ends</span>
                <input class="cal-pop-field" type="time" bind:value={endTime} />
              </label>
            {/if}
          </div>
        {/if}
        <p class="cal-pop-hint">
          {allDay ? "This event uses the whole day." : "Times use your current time zone."}
        </p>
      </div>

      <div class="cal-pop-card cal-pop-row">
        <Repeat size={14} strokeWidth={1.75} class="cal-pop-row-icon" />
        <select
          class="cal-pop-inline cal-pop-select"
          aria-label="Repeats"
          value={rrule ?? ""}
          onchange={(e) => {
            const value = e.currentTarget.value;
            rrule = value ? value : null;
          }}
        >
          {#each RRULE_PRESETS as preset (preset.label)}
            <option value={preset.value ?? ""}>{preset.label}</option>
          {/each}
        </select>
        <span class="sr-only">{rruleLabel}</span>
      </div>

      <div class="cal-pop-card">
        <div class="cal-pop-section-label">
          <Bell size={13} strokeWidth={1.75} />
          Alerts
        </div>
        {#if alarms.length === 0}
          <p class="cal-pop-hint" style="margin-top: 0.35rem">No alerts</p>
        {:else}
          <ul class="cal-pop-alarm-list">
            {#each alarms as alarm (alarm.trigger_minutes_before)}
              <li>
                <span>{alarmLabel(alarm.trigger_minutes_before)}</span>
                <button
                  type="button"
                  class="cal-pop-text"
                  onclick={() => removeAlarm(alarm.trigger_minutes_before)}
                >
                  Remove
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        <select
          class="cal-pop-field cal-pop-alarm-add"
          aria-label="Add alert"
          value=""
          onchange={(e) => {
            const minutes = Number(e.currentTarget.value);
            if (Number.isFinite(minutes) && minutes > 0) addAlarm(minutes);
            e.currentTarget.value = "";
          }}
        >
          <option value="" disabled selected>Add alert…</option>
          {#each ALARM_PRESETS.filter((row) => row.minutes > 0) as preset (preset.minutes)}
            <option value={preset.minutes}>{preset.label}</option>
          {/each}
        </select>
      </div>

      {#if !mobile}
        <div class="cal-pop-card cal-pop-row">
          <MapPin size={14} strokeWidth={1.75} class="cal-pop-row-icon" />
          <input
            class="cal-pop-inline"
            bind:value={location}
            placeholder="Add Location"
          />
        </div>
      {/if}

      <div class="cal-pop-card">
        <div class="cal-pop-section-label">
          <FileText size={13} strokeWidth={1.75} />
          Vault note
        </div>
        {#if notePath.trim()}
          <p class="cal-pop-note-path">{notePath}</p>
          <div class="cal-pop-note-actions">
            <button
              type="button"
              class="cal-pop-text"
              disabled={linking}
              onclick={() => void openLinkedNote()}
            >
              Open note
            </button>
            <button
              type="button"
              class="cal-pop-text"
              onclick={() => (notePath = "")}
            >
              Unlink
            </button>
          </div>
        {:else}
          <button
            type="button"
            class="cal-pop-link-btn"
            disabled={linking}
            onclick={() => void createLinkedNote()}
          >
            <Plus size={13} strokeWidth={2} />
            {linking ? "Creating…" : "Create linked note"}
          </button>
          <p class="cal-pop-hint">Rich notes & attachments stay in the vault.</p>
        {/if}
      </div>

      <div class="cal-pop-card">
        <textarea
          class="cal-pop-notes"
          rows="3"
          bind:value={description}
          placeholder="Short summary (optional)"
        ></textarea>
      </div>

      {#if error}
        <p class="cal-pop-error">{error}</p>
      {/if}

      {#if mobile && event && onDelete}
        <button
          type="button"
          class="cal-pop-mobile-delete"
          disabled={saving}
          onclick={() => void onDelete()}
        >
          Delete Event
        </button>
      {/if}
    </div>

    {#if !mobile}
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
          {saving ? "Saving…" : event ? "Save" : "Add"}
        </button>
      </div>
      </footer>
    {/if}
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

  .cal-pop-backdrop-mobile {
    position: fixed;
    inset: 0;
    bottom: auto;
    z-index: 70;
    height: calc(
      var(--mobile-layout-height, 100dvh) - var(--mobile-keyboard-inset, 0px)
    );
    align-items: flex-end;
    justify-content: center;
    padding: 0;
    background: rgb(var(--color-surface-950) / 0.7);
    backdrop-filter: none;
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

  .cal-pop-scroll {
    display: contents;
  }

  .cal-pop-mobile {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: min(82dvh, 46rem);
    max-height: calc(
      var(--mobile-layout-height, 100dvh) - var(--mobile-keyboard-inset, 0px) -
        max(1rem, env(safe-area-inset-top, 0px))
    );
    overflow: hidden;
    border: 1px solid rgb(var(--shell-border) / 0.62);
    border-bottom: 0;
    border-radius: 1.5rem 1.5rem 0 0;
    background: rgb(var(--shell-pane-bg));
    padding: 0;
    box-shadow: 0 -18px 52px rgb(var(--theme-shadow) / 0.32);
    backdrop-filter: none;
  }

  .cal-pop-mobile-sheet-head {
    flex-shrink: 0;
  }

  .cal-pop-mobile .cal-pop-scroll {
    display: block;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 0.75rem 1rem calc(2rem + env(safe-area-inset-bottom, 0px));
    -webkit-overflow-scrolling: touch;
  }

  .cal-pop-mobile-head {
    display: grid;
    flex-shrink: 0;
    grid-template-columns: 2.75rem minmax(0, 1fr) 2.75rem;
    align-items: center;
    gap: 0.75rem;
    padding: 0.1rem 1rem 0.45rem;
  }

  .cal-pop-mobile-head h2 {
    margin: 0;
    text-align: center;
    font-size: 1.0625rem;
    font-weight: 650;
    letter-spacing: -0.018em;
    color: rgb(var(--theme-text));
  }

  .cal-pop-mobile .cal-pop-x,
  .cal-pop-mobile-save {
    display: inline-flex;
    width: 2.75rem;
    height: 2.75rem;
    align-items: center;
    justify-content: center;
    border: 1px solid rgb(var(--shell-border) / 0.72);
    border-radius: 9999px;
    background: rgb(var(--color-surface-800) / 0.62);
    color: rgb(var(--theme-text));
  }

  .cal-pop-mobile-save {
    background: rgb(var(--color-primary-500));
    color: rgb(var(--on-primary));
  }

  .cal-pop-mobile-save:disabled {
    opacity: 0.45;
  }

  .cal-pop-kind {
    display: grid;
    flex-shrink: 0;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.15rem;
    margin: 0.15rem 1rem 0.55rem;
    border-radius: 0.8rem;
    background: rgb(var(--color-surface-800) / 0.72);
    padding: 0.16rem;
  }

  .cal-pop-kind button {
    min-height: 2.6rem;
    border-radius: 0.65rem;
    font-size: 0.875rem;
    font-weight: 600;
    color: rgb(var(--theme-text-secondary));
  }

  .cal-pop-kind button.cal-pop-kind-active {
    background: rgb(var(--color-surface-600) / 0.82);
    color: rgb(var(--theme-text));
    box-shadow: 0 1px 2px rgb(var(--theme-shadow) / 0.18);
  }

  .cal-pop-mobile .cal-pop-card {
    margin-bottom: 0.75rem;
    border: 0;
    border-radius: 1rem;
    background: rgb(var(--color-surface-800) / 0.58);
    padding: 0.8rem 0.9rem;
  }

  .cal-pop-mobile .cal-pop-card-title {
    flex-direction: column;
    align-items: stretch;
    gap: 0;
    padding-top: 0.55rem;
    padding-bottom: 0.55rem;
  }

  .cal-pop-mobile .cal-pop-title {
    min-height: 3.25rem;
    padding: 0.55rem 0.15rem !important;
    font-size: 1.1875rem;
    line-height: 1.35;
  }

  .cal-pop-mobile-location {
    min-height: 3.1rem;
    border: 0;
    border-top: 1px solid rgb(var(--shell-border) / 0.45);
    background: transparent;
    padding: 0.7rem 0.15rem 0.5rem !important;
    font-size: 1rem;
    color: rgb(var(--theme-text));
    outline: none;
    box-shadow: none;
  }

  .cal-pop-mobile-location::placeholder {
    color: rgb(var(--theme-placeholder));
  }

  .cal-pop-mobile-field-row {
    display: flex;
    min-height: 3.35rem;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border-bottom: 1px solid rgb(var(--shell-border) / 0.45);
    font-size: 0.9375rem;
    color: rgb(var(--theme-text));
  }

  .cal-pop-mobile-field-row:last-of-type {
    border-bottom: 0;
  }

  .cal-pop-mobile .cal-pop-field {
    width: auto;
    min-width: 8.5rem;
    min-height: 2.5rem;
    border-radius: 9999px;
    background: rgb(var(--color-surface-700) / 0.72);
    padding: 0.35rem 0.7rem;
    font-size: 1rem;
    text-align: right;
  }

  .cal-pop-mobile .cal-pop-alarm-add {
    width: 100%;
    text-align: left;
  }

  .cal-pop-mobile .cal-pop-row {
    min-height: 3.35rem;
  }

  .cal-pop-mobile .cal-pop-inline,
  .cal-pop-mobile .cal-pop-notes {
    font-size: 1rem;
  }

  .cal-pop-mobile .cal-pop-section-label {
    font-size: 0.75rem;
  }

  .cal-pop-mobile-delete {
    width: 100%;
    min-height: 3.25rem;
    border-radius: 1rem;
    background: rgb(var(--theme-error) / 0.1);
    font-size: 0.9375rem;
    font-weight: 600;
    color: rgb(var(--theme-error));
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

  @keyframes cal-pop-sheet-in {
    from {
      opacity: 0.6;
      transform: translateY(100%);
    }
    to {
      opacity: 1;
      transform: translateY(0);
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
    color: rgb(var(--on-primary));
  }

  .cal-pop-x {
    display: inline-flex;
    height: 2rem;
    width: 2rem;
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
    color: rgb(var(--theme-text));
    outline: none;
    box-shadow: none;
  }

  .cal-pop-title::placeholder {
    color: rgb(var(--theme-placeholder));
    font-weight: 450;
  }

  .cal-pop-title:focus {
    outline: none;
    box-shadow: none;
  }

  .cal-pop-all-day {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.35rem;
  }

  .cal-pop-switch-label {
    font-size: 0.8125rem;
    font-weight: 550;
    color: rgb(var(--theme-text-secondary));
  }

  .cal-pop-switch {
    position: relative;
    width: 3.25rem;
    height: 2.75rem;
    flex-shrink: 0;
    border-radius: 9999px;
    background: transparent;
  }

  .cal-pop-switch::before {
    content: "";
    position: absolute;
    left: 0.3rem;
    top: 0.625rem;
    width: 2.65rem;
    height: 1.5rem;
    border-radius: 9999px;
    background: rgb(var(--color-surface-700) / 0.85);
    transition: background 160ms ease;
  }

  .cal-pop-switch-on::before {
    background: rgb(var(--color-primary-500));
  }

  .cal-pop-switch-knob {
    position: absolute;
    top: 0.77rem;
    left: 0.45rem;
    width: 1.2rem;
    height: 1.2rem;
    border-radius: 9999px;
    background: rgb(var(--color-surface-50));
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.35);
    transition: transform 160ms ease;
  }

  .cal-pop-switch-on .cal-pop-switch-knob {
    transform: translateX(1.15rem);
  }

  .cal-pop-when-summary {
    font-size: 0.8125rem;
    font-weight: 550;
    letter-spacing: -0.01em;
    color: rgb(var(--theme-text));
  }

  .cal-pop-when-edit {
    display: grid;
    grid-template-columns: minmax(8.5rem, 1.2fr) repeat(2, minmax(5.75rem, 0.8fr));
    align-items: end;
    gap: 0.35rem;
    margin-top: 0.5rem;
  }

  .cal-pop-field-group {
    display: grid;
    min-width: 0;
    gap: 0.25rem;
  }

  .cal-pop-field-group > span {
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--theme-text-tertiary));
  }

  .cal-pop-field {
    width: 100%;
    min-height: 2.25rem;
    border: 0;
    border-radius: 0.4rem;
    background: rgb(var(--color-surface-800) / 0.55);
    padding: 0.2rem 0.45rem;
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--theme-text));
    outline: none;
  }

  .cal-pop-field:focus {
    background: rgb(var(--color-surface-700) / 0.65);
    box-shadow: 0 0 0 2px rgb(var(--theme-focus) / 0.42);
  }

  .cal-pop-hint {
    margin: 0.4rem 0 0;
    font-size: 0.8125rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-tertiary));
  }

  .cal-pop-row {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding-top: 0.5rem;
    padding-bottom: 0.5rem;
  }

  :global(.cal-pop-row-icon) {
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
    color: rgb(var(--theme-text));
    outline: none;
  }

  .cal-pop-inline::placeholder {
    color: rgb(var(--theme-placeholder));
  }

  .cal-pop-select {
    appearance: none;
    cursor: pointer;
  }

  .cal-pop-section-label {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted));
  }

  .cal-pop-alarm-list {
    list-style: none;
    margin: 0.4rem 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .cal-pop-alarm-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-100));
  }

  .cal-pop-alarm-add {
    width: 100%;
    margin-top: 0.45rem;
  }

  .cal-pop-note-path {
    margin: 0.4rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-200));
    word-break: break-all;
  }

  .cal-pop-note-actions {
    display: flex;
    gap: 0.75rem;
    margin-top: 0.35rem;
  }

  .cal-pop-link-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.4rem;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.8125rem;
    font-weight: 550;
    color: rgb(var(--theme-link));
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
    color: rgb(var(--theme-text));
    outline: none;
  }

  .cal-pop-notes::placeholder {
    color: rgb(var(--theme-placeholder));
  }

  .cal-pop-error {
    margin: 0 0.2rem 0.45rem;
    font-size: 0.75rem;
    color: rgb(var(--theme-error));
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
    color: rgb(var(--theme-error));
  }

  .cal-pop-save {
    min-height: 2.25rem;
    border: 0;
    border-radius: 0.5rem;
    background: rgb(var(--color-primary-500));
    padding: 0.3rem 0.85rem;
    font-size: 0.8125rem;
    font-weight: 650;
    color: rgb(var(--on-primary));
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

  @media (max-width: 28rem) {
    .cal-pop-when-edit {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .cal-pop-field-group-date {
      grid-column: 1 / -1;
    }

    .cal-pop-card-title {
      align-items: flex-start;
      flex-direction: column;
    }

    .cal-pop-all-day {
      width: 100%;
      justify-content: space-between;
    }
  }
</style>
