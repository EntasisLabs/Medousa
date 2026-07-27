export interface CalendarAlarm {
  /** Minutes before dtstart when the alert should fire. */
  trigger_minutes_before: number;
  /** RFC 5545 ACTION — typically `display`. */
  action?: string;
}

export interface CalendarEvent {
  uid: string;
  summary: string;
  description?: string | null;
  location?: string | null;
  dtstart: string;
  dtend?: string | null;
  all_day: boolean;
  rrule?: string | null;
  calendar_path: string;
  recurrence_id?: string | null;
  /** Vault-relative markdown note (`X-MEDOUSA-NOTE`). */
  note_path?: string | null;
  alarms?: CalendarAlarm[];
}

export interface CalendarListResponse {
  calendar_path: string;
  events: CalendarEvent[];
}

export interface CalendarWriteRequest {
  uid?: string | null;
  summary: string;
  description?: string | null;
  location?: string | null;
  dtstart: string;
  dtend?: string | null;
  all_day?: boolean;
  rrule?: string | null;
  calendar_path?: string | null;
  note_path?: string | null;
  alarms?: CalendarAlarm[];
}

export interface CalendarWriteResponse {
  event: CalendarEvent;
  created: boolean;
}

export interface CalendarDeleteResponse {
  uid: string;
  deleted: boolean;
  calendar_path: string;
}

export interface CalendarImportResponse {
  calendar_path: string;
  imported: number;
  updated: number;
  skipped?: number;
  warnings?: string[];
}

export interface CalendarExportResponse {
  calendar_path: string;
  content_type: string;
  ics: string;
}

/** Vault reminder row overlaid on the calendar (not a VEVENT). */
export interface CalendarReminder {
  id: string;
  title: string;
  dueDay: string;
  notePath: string;
  lineIndex: number;
  completed: boolean;
}
