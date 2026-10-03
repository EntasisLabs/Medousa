import type { ToolRunState } from "$lib/types/chat";
import { formatToolName } from "./formatTurn";

function readableName(name: string): string {
  const label = formatToolName(name).replace(/[.:/]+/g, " ").replace(/\s+/g, " ").trim();
  const short = label.length > 40 ? `${label.slice(0, 39)}…` : label;
  return short.charAt(0).toUpperCase() + short.slice(1);
}

function activity(name: string): string | null {
  const words = name.toLowerCase().split(/[._:/\s-]+/);
  const has = (...terms: string[]) => terms.some((term) => words.includes(term));
  if (has("turn") && has("begin", "finish")) return null;
  if (has("search", "discover", "find")) return "Search";
  if (has("edit", "write", "patch", "create", "delete", "remove")) return "Edit";
  if (has("read", "get", "list", "fetch", "open", "stat")) return "Read";
  if (has("test", "tests")) return "Test";
  if (has("check", "verify", "lint")) return "Check";
  if (has("build")) return "Build";
  if (has("spawn", "delegate", "invoke", "ask")) return "Delegate";
  if (has("exec", "execute", "run", "shell", "command")) return "Run";
  return readableName(name);
}

const RUNNING_LABELS: Record<string, string> = {
  Search: "Searching", Read: "Reading", Edit: "Editing", Test: "Running tests",
  Check: "Checking", Build: "Building", Delegate: "Delegating", Run: "Running commands",
};

/** A bounded activity preview; outcomes stay in the inspector. */
export function toolActivitySummary(runs: ToolRunState[]) {
  const running = runs.filter((run) => run.status === "running");
  const failed = runs.filter((run) => run.status === "failed");
  let context: string;
  if (running.length > 0) {
    const label = activity(running[running.length - 1].toolName);
    context = label ? RUNNING_LABELS[label] ?? `Running ${label}` : "Running";
  } else if (failed.length > 0) {
    context = readableName(failed[failed.length - 1].toolName);
  } else {
    const labels = [...new Set(runs.map((run) => activity(run.toolName)).filter(Boolean))];
    context = labels.slice(0, 3).join(" · ") + (labels.length > 3 ? " · …" : "");
  }
  return {
    countLabel: `${runs.length} tool${runs.length === 1 ? "" : "s"}`,
    context,
    running: running.length,
    failed: failed.length,
  };
}
