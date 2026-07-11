/**
 * Chat-side feed tail helpers for Liquid dashboard tiles (Wave B).
 * Polls daemon `feed_tail` — no canvas component subscribe / BindingResolver.
 */

import { fetchFeedTail } from "$lib/daemon";
import type { FeedEvent } from "$lib/types/environment";

export const DEFAULT_FEED_TAIL_INTERVAL_MS = 15_000;

const TONE_ALLOWLIST = new Set(["default", "accent", "success", "warn", "error"]);

function getByDotPath(root: unknown, path: string): unknown {
  if (!path) return undefined;
  let cur: unknown = root;
  for (const part of path.split(".")) {
    if (cur == null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[part];
  }
  return cur;
}

function asDisplayString(value: unknown): string | null {
  if (value == null) return null;
  if (typeof value === "string") {
    const t = value.trim();
    return t || null;
  }
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return null;
}

/** Resolve a display string from a feed event using optional field path. */
export function resolveFeedField(event: FeedEvent, field?: string): string | null {
  const raw = (field ?? "summary").trim();
  if (!raw || raw === "summary") return asDisplayString(event.summary);
  if (raw === "source") return asDisplayString(event.source);
  if (raw === "emittedAtUtc") return asDisplayString(event.emittedAtUtc);

  if (raw.startsWith("payload.")) {
    const path = raw.slice("payload.".length);
    return asDisplayString(getByDotPath(event.payload ?? null, path));
  }
  if (raw === "payload") {
    return asDisplayString(event.payload);
  }

  // Bare path: try payload first, then top-level event keys
  const fromPayload = asDisplayString(getByDotPath(event.payload ?? null, raw));
  if (fromPayload) return fromPayload;
  return asDisplayString(getByDotPath(event, raw));
}

export interface LiveTileOverrides {
  value?: string;
  delta?: string;
  tone?: string;
  hint?: string;
}

/** Map latest feed event → tile overrides (value always when resolvable). */
export function mapFeedEventToTile(
  event: FeedEvent,
  field?: string,
  opts?: { keepHint?: boolean },
): LiveTileOverrides {
  const out: LiveTileOverrides = {};
  const value = resolveFeedField(event, field);
  if (value) out.value = value;

  const payload = event.payload;
  if (payload && typeof payload === "object") {
    const delta = asDisplayString((payload as Record<string, unknown>).delta);
    if (delta) out.delta = delta;
    const toneRaw = asDisplayString((payload as Record<string, unknown>).tone)?.toLowerCase();
    if (toneRaw && TONE_ALLOWLIST.has(toneRaw)) out.tone = toneRaw;
  }

  if (!opts?.keepHint && event.emittedAtUtc) {
    out.hint = formatFeedEmittedHint(event.emittedAtUtc);
  }

  return out;
}

/** Quiet relative whisper for live tiles without a markdown hint. */
export function formatFeedEmittedHint(emittedAtUtc: string, nowMs = Date.now()): string {
  const t = Date.parse(emittedAtUtc);
  if (Number.isNaN(t)) return emittedAtUtc;
  const sec = Math.max(0, Math.floor((nowMs - t) / 1000));
  if (sec < 45) return "just now";
  if (sec < 90) return "1m ago";
  if (sec < 3600) return `${Math.floor(sec / 60)}m ago`;
  if (sec < 86400) return `${Math.floor(sec / 3600)}h ago`;
  return `${Math.floor(sec / 86400)}d ago`;
}

export async function readFeedTail(
  feedId: string,
  limit = 1,
  profileId?: string,
): Promise<FeedEvent | null> {
  const id = feedId.trim();
  if (!id) return null;
  try {
    const res = await fetchFeedTail(id, limit, profileId);
    const events = res?.events ?? [];
    if (!events.length) return null;
    return events[events.length - 1] ?? null;
  } catch {
    return null;
  }
}

export interface SubscribeFeedTailOptions {
  intervalMs?: number;
  profileId?: string;
  /** Fire immediately on subscribe (default true). */
  immediate?: boolean;
}

/**
 * Poll feed_tail while mounted. Returns unsubscribe (clears interval).
 * onEvent(null) means empty/error tail — caller keeps placeholder.
 */
export function subscribeFeedTail(
  feedId: string,
  onEvent: (event: FeedEvent | null) => void,
  options?: SubscribeFeedTailOptions,
): () => void {
  const intervalMs = options?.intervalMs ?? DEFAULT_FEED_TAIL_INTERVAL_MS;
  const immediate = options?.immediate !== false;
  let cancelled = false;
  let inFlight = false;

  const tick = async () => {
    if (cancelled || inFlight) return;
    inFlight = true;
    try {
      const event = await readFeedTail(feedId, 1, options?.profileId);
      if (!cancelled) onEvent(event);
    } finally {
      inFlight = false;
    }
  };

  if (immediate) void tick();
  const handle = setInterval(() => void tick(), intervalMs);

  return () => {
    cancelled = true;
    clearInterval(handle);
  };
}
