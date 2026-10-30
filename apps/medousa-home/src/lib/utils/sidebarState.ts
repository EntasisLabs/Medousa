export function sidebarKey(workshop: string, authority: string, profile: string, section: string): string {
  return 'medousa.sidebar.v1:' + JSON.stringify([workshop, authority, profile, section]);
}
export function readSidebarValue(key: string, fallback = false): boolean {
  try { const value: unknown = JSON.parse(localStorage.getItem(key) ?? 'null'); return typeof value === 'boolean' ? value : fallback; }
  catch { return fallback; }
}
export function writeSidebarValue(key: string, value: boolean): void {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* UI still works without storage. */ }
}
