export type TimerStatus = "idle" | "running" | "paused" | "complete";
export type TimerAction = "start" | "pause" | "resume" | "reset" | "complete";

export interface TimerStateV1 {
  schemaVersion: 1;
  status: TimerStatus;
  durationMs: number;
  remainingMs: number;
  deadlineAt?: string;
  completedAt?: string;
  notificationId?: number;
}

function boundedDuration(value: unknown, fallback: number): number {
  const numeric = typeof value === "number" ? value : Number(value);
  return Number.isFinite(numeric) && numeric >= 1_000 && numeric <= 86_400_000
    ? Math.round(numeric)
    : fallback;
}

export function createTimerState(durationMs: number): TimerStateV1 {
  const duration = boundedDuration(durationMs, 60_000);
  return { schemaVersion: 1, status: "idle", durationMs: duration, remainingMs: duration };
}

export function migrateTimerState(value: unknown, durationMs: number): TimerStateV1 {
  const fallback = createTimerState(durationMs);
  if (!value || typeof value !== "object") return fallback;
  const raw = value as Record<string, unknown>;
  const duration = boundedDuration(raw.durationMs ?? raw.duration_ms, fallback.durationMs);
  const status = ["idle", "running", "paused", "complete"].includes(String(raw.status))
    ? (raw.status as TimerStatus)
    : "idle";
  const remaining = Math.max(0, Math.min(duration, boundedDuration(raw.remainingMs ?? raw.remaining_ms, duration)));
  return {
    schemaVersion: 1,
    status,
    durationMs: duration,
    remainingMs: status === "complete" ? 0 : remaining,
    ...(typeof raw.deadlineAt === "string" ? { deadlineAt: raw.deadlineAt } : {}),
    ...(typeof raw.completedAt === "string" ? { completedAt: raw.completedAt } : {}),
    ...(typeof raw.notificationId === "number" ? { notificationId: raw.notificationId } : {}),
  };
}

export function remainingTimerMs(state: TimerStateV1, now = Date.now()): number {
  if (state.status !== "running" || !state.deadlineAt) return state.remainingMs;
  const deadline = Date.parse(state.deadlineAt);
  return Number.isFinite(deadline) ? Math.max(0, deadline - now) : state.remainingMs;
}

export function reconcileTimerState(state: TimerStateV1, now = Date.now()): TimerStateV1 {
  if (state.status !== "running") return state;
  const remainingMs = remainingTimerMs(state, now);
  if (remainingMs > 0) return { ...state, remainingMs };
  return {
    schemaVersion: 1,
    status: "complete",
    durationMs: state.durationMs,
    remainingMs: 0,
    completedAt: new Date(now).toISOString(),
  };
}

export function transitionTimer(
  prior: TimerStateV1,
  action: TimerAction,
  now = Date.now(),
): TimerStateV1 {
  const state = reconcileTimerState(prior, now);
  if (action === "reset") return createTimerState(state.durationMs);
  if (action === "complete") {
    return { ...createTimerState(state.durationMs), status: "complete", remainingMs: 0, completedAt: new Date(now).toISOString() };
  }
  if (action === "pause" && state.status === "running") {
    const remainingMs = remainingTimerMs(state, now);
    return { ...state, status: "paused", remainingMs, deadlineAt: undefined, notificationId: undefined };
  }
  if ((action === "start" && state.status === "idle") || (action === "resume" && state.status === "paused")) {
    const remainingMs = state.status === "idle" ? state.durationMs : state.remainingMs;
    return { ...state, status: "running", remainingMs, deadlineAt: new Date(now + remainingMs).toISOString(), completedAt: undefined };
  }
  return state;
}

export function formatTimerMs(milliseconds: number): string {
  const seconds = Math.max(0, Math.ceil(milliseconds / 1_000));
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  const rest = seconds % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`
    : `${minutes}:${String(rest).padStart(2, "0")}`;
}
