const PREFIX = 'medousa.chat.work-dismissed.v1:';

export function workHistoryKey(workshop: string, authority: string, session: string): string {
  return PREFIX + JSON.stringify([workshop, authority, session]);
}

export function readWorkDismissals(key: string): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key) ?? '[]');
    return Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string').slice(-500) : [];
  } catch { return []; }
}

export function writeWorkDismissals(key: string, ids: string[]): void {
  try { localStorage.setItem(key, JSON.stringify(ids.slice(-500))); }
  catch { /* A restricted/full store must not prevent inspecting work. */ }
}
