/**
 * Terminal merge for principal interactive surfaces (Phase 7A).
 *
 * Streamed tokens are canonical: terminal `final` commits tools/persist only.
 * Non-terminal events (e.g. worker ack) still replace the draft.
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
  if (streamedTrimmed) {
    return streamedBody;
  }

  const finalTrimmed = finalBody.trim();
  if (finalTrimmed) {
    return finalBody;
  }

  return streamedBody;
}
