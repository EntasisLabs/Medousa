export function workerStatusLineForColumn(column: string): string {
  switch (column) {
    case "in_flight":
    case "running":
      return "Running tools…";
    case "wrapping_up":
      return "Pulling that together…";
    case "done":
      return "Complete";
    case "blocked":
      return "Needs attention";
    default:
      return "Working in background…";
  }
}
