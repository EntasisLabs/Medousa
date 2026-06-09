/** Mirrors Rust `turn_text_heuristics` + TUI `resolve_agent_turn_content` for terminal merge. */

const WORK_IN_PROGRESS_ANYWHERE = [
  "let me ",
  "i'll ",
  "i will ",
  "i'm going to ",
  "going to ",
  "one moment",
  "one sec",
  "hang on",
  "just a sec",
  "checking ",
  "looking ",
  "working on ",
  "pulling ",
  "fetching ",
  "searching ",
  "reading ",
  "lock it in",
  "pull up ",
  "calibrate to",
  "calibrating",
];

const SHORT_ACKS = [
  "stored.",
  "stored!",
  "done.",
  "done!",
  "ok.",
  "ok!",
  "okay.",
  "okay!",
  "got it.",
  "got it!",
  "sure.",
  "sure!",
  "saved.",
  "saved!",
];

export function looksLikeInterimStatus(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed) return true;

  const lower = trimmed.toLowerCase();
  if (WORK_IN_PROGRESS_ANYWHERE.some((phrase) => lower.includes(phrase))) {
    return true;
  }
  if (SHORT_ACKS.some((ack) => lower === ack)) {
    return true;
  }

  const wordCount = trimmed.split(/\s+/).length;
  return wordCount <= 6 && !trimmed.includes("?");
}

function suffixPrefixOverlap(left: string, right: string): number {
  const max = Math.min(left.length, right.length);
  for (let size = max; size > 0; size--) {
    if (left.endsWith(right.slice(0, size))) {
      return size;
    }
  }
  return 0;
}

/**
 * Terminal merge: keep what streamed unless the stream was status-only or final extends it.
 * Non-terminal (worker ack) still replaces — async handoff unchanged.
 */
export function resolveTurnContent(
  streamedBody: string,
  finalBody: string,
  terminal: boolean,
): string {
  if (!terminal) {
    return finalBody;
  }

  const streamedTrimmed = streamedBody.trim();
  const finalTrimmed = finalBody.trim();

  if (!finalTrimmed) return streamedBody;
  if (!streamedTrimmed) return finalBody;

  if (
    looksLikeInterimStatus(streamedTrimmed) &&
    !looksLikeInterimStatus(finalTrimmed)
  ) {
    return finalBody;
  }

  if (finalTrimmed === streamedTrimmed) return streamedBody;
  if (finalTrimmed.startsWith(streamedTrimmed)) return finalBody;
  if (streamedTrimmed.startsWith(finalTrimmed)) return streamedBody;

  const overlap = suffixPrefixOverlap(streamedBody, finalBody);
  if (overlap > 0) {
    return streamedBody + finalBody.slice(overlap);
  }

  return streamedBody;
}
