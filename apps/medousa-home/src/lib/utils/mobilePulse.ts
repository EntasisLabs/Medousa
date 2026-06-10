import type { WorkCard } from "$lib/types/workspace";
import { formatCardTitle } from "$lib/utils/formatWork";

export type PulseAction =
  | { kind: "card"; cardId: string }
  | { kind: "note"; path: string }
  | { kind: "work" }
  | { kind: "chat" }
  | { kind: "settings" };

export type PulseMood = "offline" | "waiting" | "working" | "quiet";

export interface PulsePresentation {
  alive: boolean;
  mood: PulseMood;
  statusLine: string;
  eyebrow: string;
  headline: string;
  subline?: string;
  actionLabel: string;
  action: PulseAction;
  motionSummary?: string;
}

export function humanWorkColumn(column: string): string {
  switch (column) {
    case "in_flight":
      return "Running";
    case "wrapping_up":
      return "Finishing up";
    case "backlog":
      return "Queued";
    case "blocked":
      return "Needs you";
    case "done":
      return "Done";
    default:
      return column.replaceAll("_", " ");
  }
}

export function buildMotionSummary(counts: {
  inFlight: number;
  wrapping: number;
  backlog: number;
}): string | undefined {
  const parts: string[] = [];
  if (counts.inFlight > 0) {
    parts.push(`${counts.inFlight} running`);
  }
  if (counts.wrapping > 0) {
    parts.push(`${counts.wrapping} finishing`);
  }
  if (counts.backlog > 0) {
    parts.push(`${counts.backlog} queued`);
  }
  return parts.length > 0 ? parts.join(" · ") : undefined;
}

export function motionColumnCounts(cards: WorkCard[]): {
  inFlight: number;
  wrapping: number;
  backlog: number;
} {
  let inFlight = 0;
  let wrapping = 0;
  let backlog = 0;
  for (const card of cards) {
    if (card.column === "in_flight") inFlight += 1;
    else if (card.column === "wrapping_up") wrapping += 1;
    else if (card.column === "backlog") backlog += 1;
  }
  return { inFlight, wrapping, backlog };
}

export function buildPulsePresentation(input: {
  healthOk: boolean | null;
  blocked: number;
  inMotion: number;
  primaryCard: WorkCard | null;
  motionCounts: { inFlight: number; wrapping: number; backlog: number };
  journalDailyPath?: string | null;
  journalDailyTitle?: string | null;
}): PulsePresentation {
  const { inFlight, wrapping, backlog } = input.motionCounts;

  const motionSummary = buildMotionSummary({
    inFlight,
    wrapping,
    backlog,
  });

  if (input.healthOk === false) {
    return {
      alive: false,
      mood: "offline",
      statusLine: "Workshop offline",
      eyebrow: "Can't reach Medousa",
      headline: "Check your connection",
      subline: "The daemon may be stopped or on another machine.",
      actionLabel: "Workshop settings",
      action: { kind: "settings" },
    };
  }

  if (input.healthOk === null) {
    return {
      alive: false,
      mood: "quiet",
      statusLine: "Connecting…",
      eyebrow: "One moment",
      headline: "Finding your workshop",
      actionLabel: "Open chat",
      action: { kind: "chat" },
    };
  }

  if (input.blocked > 0) {
    const noun = input.blocked === 1 ? "decision" : "decisions";
    return {
      alive: true,
      mood: "waiting",
      statusLine: motionSummary ?? "Waiting on you",
      eyebrow: "Needs you",
      headline:
        input.blocked === 1
          ? "One thing needs a decision"
          : `${input.blocked} ${noun} waiting`,
      subline: "Answer when you're ready — the rest keeps moving.",
      actionLabel: "Review",
      action: { kind: "work" },
      motionSummary,
    };
  }

  if (input.primaryCard) {
    return {
      alive: true,
      mood: "working",
      statusLine: motionSummary ?? "In motion",
      eyebrow: humanWorkColumn(input.primaryCard.column),
      headline: formatCardTitle(input.primaryCard),
      subline: "Pick up where Medousa left off.",
      actionLabel: "Continue",
      action: { kind: "card", cardId: input.primaryCard.id },
      motionSummary,
    };
  }

  if (input.inMotion > 0) {
    return {
      alive: true,
      mood: "working",
      statusLine: motionSummary ?? `${input.inMotion} in motion`,
      eyebrow: "In motion",
      headline:
        input.inMotion === 1
          ? "One job is running"
          : `${input.inMotion} jobs running`,
      subline: "Nothing needs you right now.",
      actionLabel: "See work",
      action: { kind: "work" },
      motionSummary,
    };
  }

  if (input.journalDailyPath && input.journalDailyTitle) {
    return {
      alive: true,
      mood: "quiet",
      statusLine: "All clear",
      eyebrow: "Journal",
      headline: input.journalDailyTitle,
      subline: "Pick up today’s note when you’re ready.",
      actionLabel: "Open daily",
      action: { kind: "note", path: input.journalDailyPath },
    };
  }

  return {
    alive: true,
    mood: "quiet",
    statusLine: "All clear",
    eyebrow: "Quiet",
    headline: "Nothing needs you",
    subline: "Medousa is idle. Say something when inspiration strikes.",
    actionLabel: "Say hi",
    action: { kind: "chat" },
  };
}
