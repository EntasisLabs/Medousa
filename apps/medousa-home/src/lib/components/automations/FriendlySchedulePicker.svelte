<script lang="ts">
  import {
    SCHEDULE_FREQUENCY_OPTIONS,
    browserTimezone,
    cronToFriendly,
    describeFriendlySchedule,
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
  }

  let {
    mobile = false,
    cronExpr = $bindable(""),
    timezone = $bindable(""),
    optional = false,
    label = "When",
  }: Props = $props();

  let schedule = $state<FriendlyScheduleState>(cronToFriendly(""));
  let timeValue = $state("");
  let showAdvanced = $state(false);

  const barClass = $derived(mobile ? "composer-bar composer-bar-mobile" : "composer-bar");

  const frequencyOptions = $derived(
    optional
      ? [{ value: "none" as ScheduleFrequency, label: "Run manually only" }, ...SCHEDULE_FREQUENCY_OPTIONS]
      : SCHEDULE_FREQUENCY_OPTIONS,
  );

  const summary = $derived(describeFriendlySchedule(schedule, timezone));

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
</script>

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
              onchange={(event) => onWeekdayChange((event.currentTarget as HTMLSelectElement).value)}
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
