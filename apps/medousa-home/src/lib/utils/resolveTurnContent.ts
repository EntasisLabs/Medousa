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

const OUTCOME_HINTS = [
  "stability",
  "friction",
  "autonomy",
  "logic",
  "drift",
  "calibrat",
  "avec",
  "session",
  "memory",
  "node",
  "stored",
  "saved",
  "here's",
  "here is",
  "result",
  "summary",
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

export function looksLikeSubstantiveFinalAnswer(text: string): boolean {
  if (looksLikeInterimStatus(text)) return false;

  const trimmed = text.trim();
  const wordCount = trimmed.split(/\s+/).length;
  if (wordCount < 12) return false;

  const lower = trimmed.toLowerCase();
  if (wordCount >= 20) return true;
  return OUTCOME_HINTS.some((hint) => lower.includes(hint));
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

function mergeStreamedAndFinalBody(streamedBody: string, finalBody: string): string {
  const streamedTrimmed = streamedBody.trim();
  const finalTrimmed = finalBody.trim();

  if (!finalTrimmed) return streamedBody;
  if (!streamedTrimmed) return finalBody;
  if (finalTrimmed.startsWith(streamedTrimmed)) return finalBody;
  if (streamedTrimmed.startsWith(finalTrimmed)) return streamedBody;

  const overlap = suffixPrefixOverlap(streamedBody, finalBody);
  if (overlap > 0) {
    return streamedBody + finalBody.slice(overlap);
  }

  return `${streamedBody}\n\n[final synthesis]\n${finalBody}`;
}

/** Terminal events merge; non-terminal (worker ack) replace. */
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
    looksLikeSubstantiveFinalAnswer(finalTrimmed) &&
    (looksLikeInterimStatus(streamedTrimmed) ||
      !looksLikeSubstantiveFinalAnswer(streamedTrimmed))
  ) {
    return finalBody;
  }

  return mergeStreamedAndFinalBody(streamedBody, finalBody);
}
