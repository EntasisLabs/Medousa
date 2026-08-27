/**
 * Live interim / status whisper helpers for host-lane turns.
 *
 * On terminal finish we keep an explicit status whisper on the completed
 * bubble via `stageWhisper` so it survives `statusLine: null`.
 */

/**
 * Preserve interim whisper when finishing a turn.
 * Prefer a non-empty statusLine that differs from the final body; else keep
 * any existing stageWhisper.
 */
export function stageWhisperAfterFinish(
  statusLine: string | null | undefined,
  content: string | null | undefined,
  existingStageWhisper: string | null | undefined,
): string | null {
  const whisper = statusLine?.trim();
  const body = content?.trim() ?? "";
  if (whisper && whisper !== body) {
    return whisper;
  }
  const existing = existingStageWhisper?.trim();
  return existing || null;
}
