/**
 * Terminal merge for principal interactive surfaces (Phase 7A).
 *
 * Streamed tokens are canonical: terminal `final` commits tools/persist only.
 * Non-terminal events (e.g. worker ack) still replace the draft.
 */

export interface ResolveTurnContentOptions {
  /** After tool receipts, prose deltas are suppressed — prefer terminal final_text. */
  afterToolLoop?: boolean;
}

export function resolveTurnContent(
  streamedBody: string,
  finalBody: string,
  terminal: boolean,
  options?: ResolveTurnContentOptions,
): string {
  if (!terminal) {
    return finalBody;
  }

  const finalTrimmed = finalBody.trim();
  const streamedTrimmed = streamedBody.trim();

  if (options?.afterToolLoop && finalTrimmed) {
    return finalBody;
  }

  if (streamedTrimmed) {
    return streamedBody;
  }

  if (finalTrimmed) {
    return finalBody;
  }

  return streamedBody;
}
