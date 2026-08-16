import { invoke } from "@tauri-apps/api/core";
import type {
  CalendarDeleteResponse,
  CalendarExportResponse,
  CalendarImportResponse,
  CalendarListResponse,
  CalendarWriteRequest,
  CalendarWriteResponse,
} from "$lib/types/calendar";

export async function listCalendarEvents(options?: {
  from?: string;
  to?: string;
  path?: string;
}): Promise<CalendarListResponse> {
  return invoke<CalendarListResponse>("calendar_list_events", {
    from: options?.from,
    to: options?.to,
    path: options?.path,
  });
}

export async function createCalendarEvent(
  request: CalendarWriteRequest,
): Promise<CalendarWriteResponse> {
  return invoke<CalendarWriteResponse>("calendar_create_event", { request });
}

export async function updateCalendarEvent(
  uid: string,
  request: CalendarWriteRequest,
): Promise<CalendarWriteResponse> {
  return invoke<CalendarWriteResponse>("calendar_update_event", { uid, request });
}

export async function deleteCalendarEvent(
  uid: string,
  path?: string,
): Promise<CalendarDeleteResponse> {
  return invoke<CalendarDeleteResponse>("calendar_delete_event", { uid, path });
}

export async function importCalendarIcs(
  ics: string,
  path?: string,
): Promise<CalendarImportResponse> {
  return invoke<CalendarImportResponse>("calendar_import_ics", { ics, path });
}

export async function exportCalendar(path?: string): Promise<CalendarExportResponse> {
  return invoke<CalendarExportResponse>("calendar_export", { path });
}
