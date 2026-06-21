export type ScheduleFrequency =
  | "none"
  | "daily"
  | "weekdays"
  | "weekends"
  | "weekly"
  | "custom";

export interface FriendlyScheduleState {
  frequency: ScheduleFrequency;
  hour: number;
  minute: number;
  /** 0 = Sunday … 6 = Saturday */
  weekday: number;
  customCron: string;
}

const WEEKDAY_LABELS = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
] as const;

export function browserTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}

export function defaultFriendlySchedule(): FriendlyScheduleState {
  return {
    frequency: "daily",
    hour: 9,
    minute: 0,
    weekday: 1,
    customCron: "0 9 * * *",
  };
}

export function formatTime24(hour: number, minute: number): string {
  return `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
}

export function parseTime24(value: string): { hour: number; minute: number } | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
  if (!match) return null;
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  if (hour < 0 || hour > 23 || minute < 0 || minute > 59) return null;
  return { hour, minute };
}

export function friendlyToCron(state: FriendlyScheduleState): string {
  if (state.frequency === "none") return "";
  if (state.frequency === "custom") {
    return state.customCron.trim();
  }
  const minute = state.minute;
  const hour = state.hour;
  switch (state.frequency) {
    case "daily":
      return `${minute} ${hour} * * *`;
    case "weekdays":
      return `${minute} ${hour} * * 1-5`;
    case "weekends":
      return `${minute} ${hour} * * 0,6`;
    case "weekly":
      return `${minute} ${hour} * * ${state.weekday}`;
    default:
      return `${minute} ${hour} * * *`;
  }
}

export function cronToFriendly(cron: string): FriendlyScheduleState {
  const trimmed = cron.trim();
  if (!trimmed) {
    return { ...defaultFriendlySchedule(), frequency: "none", customCron: "" };
  }

  const parts = trimmed.split(/\s+/);
  if (parts.length < 5) {
    return {
      ...defaultFriendlySchedule(),
      frequency: "custom",
      customCron: trimmed,
    };
  }

  const [minPart, hourPart, dom, mon, dow] = parts;
  if (dom !== "*" || mon !== "*") {
    return {
      ...defaultFriendlySchedule(),
      frequency: "custom",
      customCron: trimmed,
    };
  }

  const minute = Number.parseInt(minPart, 10);
  const hour = Number.parseInt(hourPart, 10);
  const safeMinute = Number.isFinite(minute) ? minute : 0;
  const safeHour = Number.isFinite(hour) ? hour : 9;

  if (dow === "*") {
    return {
      frequency: "daily",
      hour: safeHour,
      minute: safeMinute,
      weekday: 1,
      customCron: trimmed,
    };
  }
  if (dow === "1-5") {
    return {
      frequency: "weekdays",
      hour: safeHour,
      minute: safeMinute,
      weekday: 1,
      customCron: trimmed,
    };
  }
  if (dow === "0,6" || dow === "6,0") {
    return {
      frequency: "weekends",
      hour: safeHour,
      minute: safeMinute,
      weekday: 0,
      customCron: trimmed,
    };
  }
  if (/^[0-6]$/.test(dow)) {
    return {
      frequency: "weekly",
      hour: safeHour,
      minute: safeMinute,
      weekday: Number.parseInt(dow, 10),
      customCron: trimmed,
    };
  }

  return {
    frequency: "custom",
    hour: safeHour,
    minute: safeMinute,
    weekday: 1,
    customCron: trimmed,
  };
}

export function describeFriendlySchedule(
  state: FriendlyScheduleState,
  timezone?: string,
): string {
  if (state.frequency === "none") return "Run manually only";
  const time = formatTime12(state.hour, state.minute);
  const tz = timezone?.trim() ? ` (${timezone.trim()})` : "";
  switch (state.frequency) {
    case "daily":
      return `Every day at ${time}${tz}`;
    case "weekdays":
      return `Weekdays at ${time}${tz}`;
    case "weekends":
      return `Weekends at ${time}${tz}`;
    case "weekly":
      return `Every ${WEEKDAY_LABELS[state.weekday] ?? "week"} at ${time}${tz}`;
    case "custom":
      return state.customCron.trim() ? `Custom · ${state.customCron.trim()}${tz}` : "Custom schedule";
    default:
      return `Every day at ${time}${tz}`;
  }
}

export function formatTime12(hour: number, minute: number): string {
  const suffix = hour >= 12 ? "PM" : "AM";
  const hour12 = hour % 12 === 0 ? 12 : hour % 12;
  return `${hour12}:${String(minute).padStart(2, "0")} ${suffix}`;
}

export function weekdayLabel(weekday: number): string {
  return WEEKDAY_LABELS[weekday] ?? "Monday";
}

export const SCHEDULE_FREQUENCY_OPTIONS: Array<{
  value: ScheduleFrequency;
  label: string;
}> = [
  { value: "daily", label: "Every day" },
  { value: "weekdays", label: "Weekdays" },
  { value: "weekends", label: "Weekends" },
  { value: "weekly", label: "Weekly" },
  { value: "custom", label: "Custom (cron)" },
];
