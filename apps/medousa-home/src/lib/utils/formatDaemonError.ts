/** Turn raw daemon/Tauri errors into short UI copy. */
export function formatDaemonErrorSummary(raw: unknown): string {
  const message = normalizeDaemonError(raw);

  if (message.includes("Session not found")) {
    return "Workshop database reconnecting — telemetry may be briefly unavailable";
  }
  if (/failed to fetch|network|connection refused|ECONNREFUSED/i.test(message)) {
    return "Cannot reach the workshop backend";
  }
  if (/500|502|503|504/.test(message)) {
    return "Workshop busy — telemetry will retry quietly";
  }
  if (/404/.test(message)) {
    return "Telemetry endpoint unavailable";
  }
  if (message.length > 96) {
    return `${message.slice(0, 93)}…`;
  }
  if (message) return message;
  return "Telemetry unavailable";
}

export function normalizeDaemonError(raw: unknown): string {
  const message =
    raw instanceof Error ? raw.message : typeof raw === "string" ? raw : String(raw);

  return message
    .replace(/^daemon GET [^\s]+ failed \(\d+[^)]*\):\s*/i, "")
    .replace(/^daemon POST [^\s]+ failed \(\d+[^)]*\):\s*/i, "")
    .replace(/^medousa daemon error:\s*/i, "")
    .replace(/^port failure:\s*/i, "")
    .trim();
}
