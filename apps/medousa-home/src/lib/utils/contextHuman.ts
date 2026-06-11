import type { ContextRecallKind } from "$lib/types/context";
import type { LocusAvecSnapshot } from "$lib/types/locus";

const TIER_LABELS: Record<string, string> = {
  raw: "Moment",
  daily: "Daily",
  weekly: "Weekly",
  monthly: "Monthly",
  quarterly: "Quarterly",
  yearly: "Yearly",
};

const RECALL_KIND_LABELS: Record<ContextRecallKind, string> = {
  claim: "Remembers",
  contact: "Person",
  relationship: "Connection",
  persona: "Persona",
  user: "You",
};

export function looksLikeOpaqueId(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return true;
  if (
    /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(
      trimmed,
    )
  ) {
    return true;
  }
  const alnum = trimmed.replace(/[^a-z0-9]/gi, "");
  if (alnum.length >= 24 && /^[0-9a-f]+$/i.test(alnum)) return true;
  if (trimmed.length >= 28 && !/\s/.test(trimmed) && /[0-9a-f-]{20,}/i.test(trimmed)) {
    return true;
  }
  return false;
}

export function sessionDisplayName(
  sessionId: string,
  sessionLabels: Record<string, string> = {},
): string {
  const trimmed = sessionId.trim();
  const label = sessionLabels[trimmed]?.trim();
  if (label && label !== trimmed && !looksLikeOpaqueId(label)) return label;
  return humanizeSessionId(trimmed);
}

export function sessionMapLabel(
  sessionId: string,
  sessionLabels: Record<string, string>,
  firstTimestamp?: string,
): string {
  const trimmed = sessionId.trim();
  const label = sessionLabels[trimmed]?.trim();
  if (label && label !== trimmed && !looksLikeOpaqueId(label)) return label;

  const base = sessionDisplayName(trimmed, sessionLabels);
  if (!looksLikeOpaqueId(base) && base !== trimmed) return base;

  if (firstTimestamp) {
    const when = formatContextWhen(firstTimestamp);
    const datePart = when.split(" · ")[0] ?? when;
    return `Session · ${datePart}`;
  }
  return "Session";
}

export function humanMomentTitle(node: {
  context_summary: string;
  timestamp: string;
  sync_key?: string;
}): string {
  const summary = node.context_summary.trim();
  if (summary && !looksLikeOpaqueId(summary) && summary !== node.sync_key?.trim()) {
    return summary;
  }
  return `Untitled moment · ${formatContextWhen(node.timestamp)}`;
}

function humanizeSessionId(sessionId: string): string {
  if (!sessionId) return "Unknown session";
  if (looksLikeOpaqueId(sessionId)) return "Session";
  return sessionId
    .split(/[-_]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export function tierHumanLabel(tier: string): string {
  const key = tier.trim().toLowerCase();
  return TIER_LABELS[key] ?? tier.charAt(0).toUpperCase() + tier.slice(1).toLowerCase();
}

export function recallKindHumanLabel(kind: ContextRecallKind): string {
  return RECALL_KIND_LABELS[kind];
}

export function formatContextWhen(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return timestamp;

  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const startOfDate = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const diffDays = Math.round(
    (startOfToday.getTime() - startOfDate.getTime()) / (24 * 60 * 60 * 1000),
  );
  const time = date.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });

  if (diffDays === 0) return `Today · ${time}`;
  if (diffDays === 1) return `Yesterday · ${time}`;
  if (diffDays > 1 && diffDays < 7) {
    return `${date.toLocaleDateString(undefined, { weekday: "long" })} · ${time}`;
  }

  const datePart = date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    ...(date.getFullYear() !== now.getFullYear() ? { year: "numeric" as const } : {}),
  });
  return `${datePart} · ${time}`;
}

export function threadMetaLine(
  sessionId: string,
  timestamp: string,
  tier: string,
  sessionLabels: Record<string, string> = {},
): string {
  return `${sessionDisplayName(sessionId, sessionLabels)} · ${formatContextWhen(timestamp)} · ${tierHumanLabel(tier)}`;
}

export function extractThreadMemory(raw: string, fallbackSummary = ""): string {
  const summary = fallbackSummary.trim();
  const trimmed = raw.trim();

  if (!trimmed) {
    return summary || "A moment she kept from this session.";
  }

  try {
    const parsed = JSON.parse(trimmed) as { context_summary?: string; raw?: string };
    if (typeof parsed.context_summary === "string" && parsed.context_summary.trim()) {
      return parsed.context_summary.trim();
    }
    if (typeof parsed.raw === "string") {
      return extractThreadMemory(parsed.raw, summary);
    }
  } catch {
    // STTP text body
  }

  const vibeMatch = trimmed.match(/vibe_signature\([^)]*\):\s*"([^"]+)"/i);
  if (vibeMatch?.[1]?.trim()) return vibeMatch[1].trim();

  const humanFields = extractSttpHumanFields(trimmed);
  if (humanFields.length > 0) {
    return humanFields.slice(0, 2).join(" · ");
  }

  if (summary) return summary;
  return "A moment she kept from this session.";
}

function extractSttpHumanFields(raw: string): string[] {
  const fields: string[] = [];
  const contentStart = raw.indexOf("◈");
  const slice = contentStart >= 0 ? raw.slice(contentStart) : raw;

  const labeledRe =
    /(?:focus|topic|decision|summary|narrative|context|note|subject|theme|intent|goal|task|issue|bug|request|role|primary_rule|fact_grounding)\([^)]*\):\s*"([^"]{6,})"/gi;
  let match: RegExpExecArray | null;
  while ((match = labeledRe.exec(slice)) !== null) {
    fields.push(match[1].trim());
  }

  const quoteRe = /"([^"]{14,140})"/g;
  while ((match = quoteRe.exec(slice)) !== null) {
    const value = match[1].trim();
    if (looksTechnical(value)) continue;
    fields.push(value);
  }

  return [...new Set(fields)];
}

function looksTechnical(value: string): boolean {
  const lower = value.toLowerCase();
  return (
    lower.includes("sttp") ||
    lower.includes("session_id") ||
    lower.includes("schema_version") ||
    lower.includes("medousa-system") ||
    /^\d{4}-\d{2}-\d{2}/.test(value) ||
    value.includes("⏣") ||
    value.includes("⊕") ||
    value.includes("◈")
  );
}

export function postureHumanFeel(avec: LocusAvecSnapshot): string {
  const { stability, friction, logic, autonomy } = avec;

  if (stability > 0.85 && friction < 0.35) {
    return "Grounded — you had your footing in this session.";
  }
  if (friction > 0.52) {
    return "Friction in the room — tension showed up in how you moved.";
  }
  if (logic > 0.9 && autonomy > 0.85) {
    return "Clear and self-directed — you knew what you were doing.";
  }
  if (stability < 0.68) {
    return "Scattered — a chaotic stretch, hard to settle.";
  }
  if (autonomy > 0.88) {
    return "In charge — you drove the thread.";
  }
  if (logic > 0.88 && friction < 0.4) {
    return "Focused — reasoning stayed sharp.";
  }
  return "Mixed signals — a lived-in session, not a clean line.";
}

export function postureListWhisper(avec: LocusAvecSnapshot, threadCount: number): string {
  const feel = postureHumanFeel(avec).replace(/\.$/, "");
  return `${feel} · ${threadCount} moment${threadCount === 1 ? "" : "s"}`;
}
