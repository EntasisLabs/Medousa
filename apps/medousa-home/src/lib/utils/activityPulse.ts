/** How long a fresh feed event keeps the pulse “hot”. */
export const ACTIVITY_HOT_MS = 4_000;

export function isActivityFeedHot(
  latestTimestampUtc: string | null | undefined,
  now = Date.now(),
  windowMs = ACTIVITY_HOT_MS,
): boolean {
  if (!latestTimestampUtc) return false;
  const t = Date.parse(latestTimestampUtc);
  if (Number.isNaN(t)) return false;
  return now - t <= windowMs;
}

export function truncateActivityLabel(label: string, maxChars = 36): string {
  const trimmed = label.trim();
  if (trimmed.length <= maxChars) return trimmed;
  return `${trimmed.slice(0, Math.max(1, maxChars - 1)).trimEnd()}…`;
}
