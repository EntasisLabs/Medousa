/**
 * Live interim / status whisper helpers for host-lane turns.
 *
 * Between tool rounds the server archives draft prose then clears the stream
 * buffer (`scratch_reset`). On terminal finish we keep that whisper on the
 * completed bubble via `stageWhisper` so it survives `statusLine: null`.
 */

/** Promote live draft into status whisper when scratch is cleared between tool rounds. */
export function statusLineAfterScratchReset(
  content: string | null | undefined,
  statusLine: string | null | undefined,
): string | null {
  const draft = content?.trim();
  if (draft) return draft;
  const existing = statusLine?.trim();
  return existing || null;
}

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
