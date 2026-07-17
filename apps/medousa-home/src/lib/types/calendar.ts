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
