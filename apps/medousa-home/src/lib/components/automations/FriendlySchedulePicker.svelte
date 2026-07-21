<script lang="ts">
  import {
    SCHEDULE_FREQUENCY_OPTIONS,
    browserTimezone,
    cronToFriendly,
    describeFriendlySchedule,
    formatTime12,
    formatTime24,
    friendlyToCron,
    parseTime24,
    weekdayLabel,
    type FriendlyScheduleState,
    type ScheduleFrequency,
  } from "$lib/utils/friendlySchedule";

  interface Props {
    mobile?: boolean;
    cronExpr: string;
    timezone: string;
    /** When true, user can pick "Run manually only" (empty cron). */
    optional?: boolean;
    label?: string;
    /** Progressive disclosure for liquid popovers. */
    liquid?: boolean;
    /** Fired when open/step changes so hosts can re-place popovers. */
    onLayoutChange?: () => void;
  }

  let {
    mobile = false,
    cronExpr = $bindable(""),
    timezone = $bindable(""),
    optional = false,
    label = "When",
    liquid = false,
    onLayoutChange,
  }: Props = $props();

  let schedule = $state<FriendlyScheduleState>(cronToFriendly(""));
  let timeValue = $state("");
  let showAdvanced = $state(false);
  let timeInputEl: HTMLInputElement | undefined = $state();

  /** One question at a time — never a stacked form. */
  let disclosed = $state(false);
  let step = $state<"freq" | "time" | "zone">("freq");

  const TIME_PRESETS = [
    { hour: 7, minute: 0, label: "7:00 AM" },
    { hour: 9, minute: 0, label: "9:00 AM" },
    { hour: 12, minute: 0, label: "12:00 PM" },
    { hour: 17, minute: 0, label: "5:00 PM" },
    { hour: 21, minute: 0, label: "9:00 PM" },
  ] as const;

  const barClass = $derived(mobile ? "composer-bar composer-bar-mobile" : "composer-bar");

  const frequencyOptions = $derived(
    optional
      ? [{ value: "none" as ScheduleFrequency, label: "Run manually only" }, ...SCHEDULE_FREQUENCY_OPTIONS]
      : SCHEDULE_FREQUENCY_OPTIONS,
  );

  const summary = $derived(describeFriendlySchedule(schedule, timezone));
  const timeLabel = $derived(formatTime12(schedule.hour, schedule.minute));

  $effect(() => {
    const next = cronToFriendly(cronExpr);
    schedule = next;
    timeValue = formatTime24(next.hour, next.minute);
    showAdvanced = next.frequency === "custom";
  });

  $effect(() => {
    if (!timezone.trim()) {
      timezone = browserTimezone();
    }
  });

  $effect(() => {
    disclosed;
    step;
    onLayoutChange?.();
  });

  function applySchedule(next: FriendlyScheduleState) {
    schedule = next;
    timeValue = formatTime24(next.hour, next.minute);
    showAdvanced = next.frequency === "custom";
    cronExpr =
      next.frequency === "custom" ? next.customCron.trim() : friendlyToCron(next);
  }

  function setFrequency(frequency: ScheduleFrequency) {
    applySchedule({
      ...schedule,
      frequency,
      customCron: frequency === "custom" ? cronExpr : schedule.customCron,
    });
  }

  function onTimeChange(value: string) {
    timeValue = value;
    const parsed = parseTime24(value);
    if (!parsed) return;
    applySchedule({ ...schedule, hour: parsed.hour, minute: parsed.minute });
  }

  function onWeekdayChange(value: string) {
    applySchedule({ ...schedule, weekday: Number.parseInt(value, 10) || 1 });
  }

  function onCustomCronChange(value: string) {
    schedule = { ...schedule, frequency: "custom", customCron: value };
    showAdvanced = true;
    cronExpr = value;
  }

  function toggleDisclose() {
    if (disclosed) {
      disclosed = false;
      return;
    }
    disclosed = true;
    step = "freq";
  }

  function pickFrequency(frequency: ScheduleFrequency) {
    setFrequency(frequency);
    if (frequency === "none") {
      disclosed = false;
      return;
    }
    if (frequency === "custom") {
      showAdvanced = true;
      step = "zone";
      return;
    }
    step = "time";
  }

  function finishTime() {
    step = "zone";
  }

  function finishZone() {
    disclosed = false;
  }

  function openTimePicker() {
    const el = timeInputEl;
    if (!el) return;
    try {
      if (typeof el.showPicker === "function") {
        el.showPicker();
        return;
      }
    } catch {
      /* fall through */
    }
    el.focus();
    el.click();
  }

  function nudgeMinutes(delta: number) {
    const total = (schedule.hour * 60 + schedule.minute + delta + 24 * 60) % (24 * 60);
    applySchedule({
      ...schedule,
      hour: Math.floor(total / 60),
      minute: total % 60,
    });
  }

  function pickPreset(hour: number, minute: number) {
    applySchedule({ ...schedule, hour, minute });
  }
</script>

{#if liquid}
  <div class="schedule-disclose">
    <button
      type="button"
      class="schedule-disclose-summary"
      aria-expanded={disclosed}
      aria-label="Edit when it runs"
      onclick={toggleDisclose}
    >
      <span class="schedule-disclose-summary-text">{summary}</span>
      <span class="schedule-disclose-chevron" aria-hidden="true">{disclosed ? "▴" : "▾"}</span>
    </button>

    {#if disclosed}
      <div class="schedule-disclose-panel">
        {#if step === "freq"}
          <p class="schedule-disclose-prompt">How often?</p>
          <div class="schedule-disclose-choices" role="listbox" aria-label="Schedule frequency">
            {#each frequencyOptions as option (option.value)}
              <button
                type="button"
                class="schedule-disclose-choice {schedule.frequency === option.value
                  ? 'schedule-disclose-choice-active'
                  : ''}"
                role="option"
                aria-selected={schedule.frequency === option.value}
                onclick={() => pickFrequency(option.value)}
              >
                {option.label}
              </button>
            {/each}
          </div>
        {:else if step === "time"}
          <p class="schedule-disclose-prompt">What time?</p>
          {#if schedule.frequency === "weekly"}
            <div class="schedule-disclose-choices schedule-disclose-choices-compact" role="listbox">
              {#each [0, 1, 2, 3, 4, 5, 6] as day (day)}
                <button
                  type="button"
                  class="schedule-disclose-choice {schedule.weekday === day
                    ? 'schedule-disclose-choice-active'
                    : ''}"
                  onclick={() => onWeekdayChange(String(day))}
                >
                  {weekdayLabel(day)}
                </button>
              {/each}
            </div>
          {/if}
          <div class="schedule-disclose-time-row">
            <div class="schedule-disclose-time">
              <button
                type="button"
                class="schedule-disclose-nudge"
                aria-label="Earlier"
                onclick={() => nudgeMinutes(-30)}
              >
                −
              </button>
              <button
                type="button"
                class="schedule-disclose-time-face"
                aria-label="Choose time, currently {timeLabel}"
                onclick={openTimePicker}
              >
                {timeLabel}
              </button>
              <button
                type="button"
                class="schedule-disclose-nudge"
                aria-label="Later"
                onclick={() => nudgeMinutes(30)}
              >
                +
              </button>
              <input
                bind:this={timeInputEl}
                class="schedule-disclose-time-native"
                type="time"
                value={timeValue}
                oninput={(event) => onTimeChange((event.currentTarget as HTMLInputElement).value)}
                tabindex="-1"
                aria-hidden="true"
              />
            </div>
            <button type="button" class="schedule-disclose-next" onclick={finishTime}>
              Next
            </button>
          </div>
          <div class="schedule-disclose-presets" role="group" aria-label="Common times">
            {#each TIME_PRESETS as preset (preset.label)}
              <button
                type="button"
                class="schedule-disclose-preset {schedule.hour === preset.hour &&
                schedule.minute === preset.minute
                  ? 'schedule-disclose-preset-active'
                  : ''}"
                onclick={() => pickPreset(preset.hour, preset.minute)}
              >
                {preset.label}
              </button>
            {/each}
          </div>
        {:else}
          <p class="schedule-disclose-prompt">
            {schedule.frequency === "custom" ? "Cron & timezone" : "Timezone"}
          </p>
          {#if schedule.frequency === "custom"}
            <input
              class="schedule-disclose-plain font-mono"
              value={schedule.customCron}
              oninput={(event) =>
                onCustomCronChange((event.currentTarget as HTMLInputElement).value)}
              placeholder="0 9 * * *"
              spellcheck="false"
              aria-label="Cron expression"
            />
          {/if}
          <input
            class="schedule-disclose-plain"
            bind:value={timezone}
            placeholder={browserTimezone()}
            autocapitalize="off"
            autocorrect="off"
            spellcheck="false"
            aria-label="Timezone"
          />
          <button type="button" class="schedule-disclose-next mt-1" onclick={finishZone}>
            Done
          </button>
        {/if}
      </div>
    {/if}
  </div>
{:else}
  <div class="friendly-schedule {mobile ? 'friendly-schedule-mobile' : ''}">
    <p class="cron-field-label">{label}</p>
    <p class="friendly-schedule-summary">{summary}</p>

    <div class="friendly-schedule-frequency" role="group" aria-label="Schedule frequency">
      {#each frequencyOptions as option (option.value)}
        <button
          type="button"
          class="friendly-schedule-chip {schedule.frequency === option.value
            ? 'friendly-schedule-chip-active'
            : ''}"
          onclick={() => setFrequency(option.value)}
        >
          {option.label}
        </button>
      {/each}
    </div>

    {#if schedule.frequency !== "none" && schedule.frequency !== "custom"}
      <div class="friendly-schedule-row">
        {#if schedule.frequency === "weekly"}
          <label class="friendly-schedule-field">
            <span class="friendly-schedule-field-label">Day</span>
            <div class="{barClass} cron-field-bar cron-field-bar-compact">
              <select
                class="cron-field-input"
                value={String(schedule.weekday)}
                onchange={(event) =>
                  onWeekdayChange((event.currentTarget as HTMLSelectElement).value)}
                aria-label="Day of week"
              >
                {#each [0, 1, 2, 3, 4, 5, 6] as day (day)}
                  <option value={day}>{weekdayLabel(day)}</option>
                {/each}
              </select>
            </div>
          </label>
        {/if}

        <label class="friendly-schedule-field">
          <span class="friendly-schedule-field-label">Time</span>
          <div class="{barClass} cron-field-bar cron-field-bar-compact">
            <input
              class="cron-field-input"
              type="time"
              value={timeValue}
              oninput={(event) => onTimeChange((event.currentTarget as HTMLInputElement).value)}
              aria-label="Run time"
            />
          </div>
        </label>
      </div>
    {/if}

    {#if schedule.frequency !== "none"}
      <label class="friendly-schedule-field">
        <span class="friendly-schedule-field-label">Timezone</span>
        <div class="{barClass} cron-field-bar cron-field-bar-compact">
          <input
            class="cron-field-input"
            bind:value={timezone}
            placeholder={browserTimezone()}
            autocapitalize="off"
            autocorrect="off"
            spellcheck="false"
            aria-label="Timezone"
          />
        </div>
        <p class="friendly-schedule-hint">Usually your local timezone — e.g. America/New_York</p>
      </label>
    {/if}

    {#if schedule.frequency === "custom" || showAdvanced}
      <label class="friendly-schedule-field">
        <span class="friendly-schedule-field-label">Advanced cron</span>
        <div class="{barClass} cron-field-bar cron-field-bar-compact">
          <input
            class="cron-field-input font-mono text-sm"
            value={schedule.customCron}
            oninput={(event) => onCustomCronChange((event.currentTarget as HTMLInputElement).value)}
            placeholder="0 9 * * *"
            spellcheck="false"
            aria-label="Cron expression"
          />
        </div>
      </label>
    {:else if schedule.frequency !== "none"}
      <button
        type="button"
        class="workshop-text-action friendly-schedule-advanced-toggle text-[11px]"
        onclick={() => {
          showAdvanced = true;
          applySchedule({
            ...schedule,
            frequency: "custom",
            customCron: cronExpr || friendlyToCron(schedule),
          });
        }}
      >
        Advanced cron…
      </button>
    {/if}
  </div>
{/if}
