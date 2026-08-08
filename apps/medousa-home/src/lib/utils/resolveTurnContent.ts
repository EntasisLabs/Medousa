/**
 * Terminal merge for principal interactive surfaces (Phase 7A).
 *
 * Terminal `final_text` is canonical. Streamed tokens are presentation-only and
 * may still be queued when the terminal event is committed.
 */

export function resolveTurnContent(
  streamedBody: string,
  finalBody: string,
  terminal: boolean,
): string {
  if (!terminal) {
    return finalBody;
  }

  const finalTrimmed = finalBody.trim();
  const streamedTrimmed = streamedBody.trim();

  if (finalTrimmed) {
    return finalBody;
  }
  return streamedTrimmed ? streamedBody : finalBody;
}
