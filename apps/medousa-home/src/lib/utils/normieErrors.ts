/** Map daemon/transport raw errors to operator-facing copy (no jargon). */

export const MAX_MEDIA_REFS_PER_TURN = 5;
export const MAX_MEDIA_UPLOAD_MB = 25;

const GENERIC_TURN =
  "Medousa couldn't complete this turn. Check the error details before retrying.";

export function friendlyUserError(raw: string, context?: { fileName?: string }): string {
  const trimmed = raw.trim();
  if (!trimmed) return GENERIC_TURN;

  // A lease/storage failure may mention a missing file or permission denial;
  // it is not an attachment error, even on surfaces that accept uploads.
  const lower = trimmed.toLowerCase();
  if (
    lower.includes("cannot acquire coder authority") ||
    lower.includes("coder forge lease rejected") ||
    (lower.includes("invalid transition") && lower.includes("cannot begin attempt")) ||
    lower.includes("no space left on device") ||
    lower.includes("disk quota exceeded") ||
    lower.includes("undertaking workspace is held by an executor outside coder shared-space coordination")
  ) {
    return friendlyTurnError(trimmed);
  }

  const media = friendlyMediaUploadError(trimmed, context?.fileName);
  if (media !== trimmed) return media;

  return friendlyTurnError(trimmed);
}

export function friendlyMediaUploadError(raw: string, fileName?: string): string {
  const text = raw.trim();
  const lower = text.toLowerCase();
  const prefix = fileName ? `"${fileName}" — ` : "";

  if (lower === "empty file" || lower.includes("empty file")) {
    return `${prefix}That file looks empty. Pick a different one.`;
  }
  if (
    lower.includes("exceeds max size") ||
    lower.includes("file too large") ||
    lower.includes("entity too large")
  ) {
    return `${prefix}That file is too large — max ${MAX_MEDIA_UPLOAD_MB} MB per attachment.`;
  }
  if (lower.includes("mime type not allowed") || (fileName && lower.includes("not allowed"))) {
    return `${prefix}That file type isn't supported. Try PDF, images, spreadsheets, or text files.`;
  }
  if (lower.includes("too many attachments")) {
    return `You can attach up to ${MAX_MEDIA_REFS_PER_TURN} files per message. Remove one and try again.`;
  }
  if (lower.includes("unknown media_id")) {
    return "That attachment expired. Remove it and attach the file again.";
  }
  if (
    lower.includes("artifact not found") ||
    lower.includes("artifact_id is required")
  ) {
    return "That presentation couldn't be loaded. Try asking Medousa to show it again.";
  }
  if (
    (lower.includes("failed to read") && (fileName || lower.includes("file"))) ||
    lower.includes("no such file") ||
    (fileName && lower.includes("not found") && !lower.includes("model"))
  ) {
    return `${prefix}Couldn't read that file. It may have moved — pick it again.`;
  }
  if (
    lower.includes("cannot reach") ||
    lower.includes("connection refused") ||
    lower.includes("failed to fetch") ||
    lower.includes("network error") ||
    lower.includes("engine did not become ready")
  ) {
    return "Medousa isn't connected right now. Check that your workshop is running, then try again.";
  }
  if (lower.includes("permission denied") || lower.includes("access denied")) {
    return `${prefix}Medousa doesn't have permission to open that file.`;
  }

  return text;
}

// Match explicit HTTP status wrappers, not incidental IDs or byte counts.
function hasHttpStatus(text: string, code: string): boolean {
  return new RegExp(`(?:http |http error: |status |status: |status code |status code: |status_code=|status=|statuscode\\()\\s*${code}(?![0-9])`).test(text);
}

export function friendlyTurnError(raw: string): string {
  const text = raw.trim();
  if (!text) return GENERIC_TURN;
  const lower = text.toLowerCase();

  if (lower.includes("undertaking workspace is held by an executor outside coder shared-space coordination")) {
    return "This project's workspace is in use by another executor. Finish or stop that work before starting Coder.";
  }
  // Local lifecycle/storage blockers must win over incidental provider text.
  if (lower.includes("invalid transition") && lower.includes("cannot begin attempt")) {
    if (lower.includes(" is accepted,")) {
      return "Coder couldn't start because this work is already accepted. Create a new undertaking for the next changes; retrying this assignment won't reopen it.";
    }
    if (lower.includes(" is discarded,")) {
      return "Coder couldn't start because this work was discarded. Create a new undertaking to continue.";
    }
    if (lower.includes(" is awaiting_review,")) {
      return "Coder couldn't start because this work is awaiting review. Review it or request changes before assigning more work.";
    }
    return "Coder couldn't start because this work isn't executable in its current state. Inspect the undertaking's state before assigning more work.";
  }
  if (lower.includes("no space left on device") || lower.includes("disk quota exceeded")) {
    return "Medousa couldn't save runtime data because the workshop disk is full. Free disk space, then check the work state before retrying.";
  }
  if (lower.includes("cannot acquire coder authority") || lower.includes("coder forge lease rejected")) {
    return "Coder couldn't obtain permission to execute this work. Check the undertaking's state and active executor in the runtime details before retrying.";
  }
  if (lower.includes("chatgpt") && lower.includes("sign-in")) {
    return "Reconnect ChatGPT in Settings → Medousa Agent → Models, then submit a new request.";
  }
  if (lower.includes("session") && lower.includes("not found")) {
    return "That conversation isn't available on this workshop. Open another conversation or start a new one.";
  }

  if (lower.includes("cancelled") || lower.includes("canceled")) {
    return "Turn cancelled.";
  }
  if (
    lower.includes("forge command queue is full") ||
    lower.includes("forge queued-byte budget is exhausted") ||
    lower.includes("repository-lane registry is full")
  ) {
    return "Medousa's work queue is full. Wait for active work to finish, then try again.";
  }
  if (lower.includes("estimated retained bytes") && lower.includes("class budget")) {
    return "This operation exceeds Medousa's per-operation data limit. See the error details for the operation and limit.";
  }
  if (
    hasHttpStatus(lower, "401") ||
    hasHttpStatus(lower, "403") ||
    lower.includes("invalid api key") ||
    lower.includes("incorrect api key") ||
    lower.includes("authentication") ||
    lower.includes("unauthorized")
  ) {
    return "The model provider rejected the API key. Check Settings → Medousa Agent and try again.";
  }
  if (
    hasHttpStatus(lower, "429") ||
    lower.includes("rate limit") ||
    lower.includes("too many requests") ||
    lower.includes("quota")
  ) {
    return "The model provider is rate-limiting requests. Wait a moment and try again.";
  }
  if (
    hasHttpStatus(lower, "404") ||
    lower.includes("model not found") ||
    (lower.includes("model") && lower.includes("does not exist")) ||
    lower.includes("unknown model") ||
    lower.includes("invalid model")
  ) {
    return "That model isn't available right now. Choose a different model in Settings.";
  }
  if (
    lower.includes("timeout") ||
    lower.includes("timed out") ||
    lower.includes("deadline exceeded")
  ) {
    return "The turn timed out. Check whether any work completed before retrying.";
  }
  if (
    lower.includes("connection") ||
    lower.includes("transport") ||
    lower.includes("unavailable") ||
    hasHttpStatus(lower, "502") ||
    hasHttpStatus(lower, "503") ||
    hasHttpStatus(lower, "504") ||
    (lower.includes("queue") && (lower.includes("busy") || lower.includes("full")))
  ) {
    return "Couldn't reach the model provider. Try again in a moment.";
  }
  if (lower.includes("session_id") && lower.includes("required")) {
    return "Start or select a chat session before sending a message.";
  }
  if (lower.includes("prompt") && lower.includes("required")) {
    return "Type a message or attach a file before sending.";
  }
  if (lower.includes("too many attachments")) {
    return `You can attach up to ${MAX_MEDIA_REFS_PER_TURN} files per message. Remove one and try again.`;
  }
  if (lower.includes("vision") && (lower.includes("required") || lower.includes("profile"))) {
    return "Configure a vision model in Settings → Medousa Agent before sending images.";
  }
  if (
    lower.includes("sse stream ended unexpectedly") ||
    lower.includes("could not reattach")
  ) {
    return "Lost connection mid-turn. Medousa will try to reconnect — send again if nothing appears.";
  }
  if (lower.startsWith("http ")) {
    return "Couldn't reach your workshop. Check the connection and try again.";
  }

  // Already human copy from the engine — pass through.
  if (
    !lower.includes("orchestrator=") &&
    !lower.startsWith("tool=") &&
    !text.startsWith("◈") &&
    text.length < 240 &&
    !/\b0x[0-9a-f]+\b/i.test(text)
  ) {
    return text;
  }

  return GENERIC_TURN;
}

/** Optional-route / older-engine probes — hide or soften, don't show raw HTTP. */
export function isMissingCapabilityError(raw: string): boolean {
  const lower = raw.trim().toLowerCase();
  if (!lower) return false;
  return (
    lower.includes("404") ||
    lower.includes("not found") ||
    lower.includes("unknown route") ||
    lower.includes("no route") ||
    lower.includes("unsupported") ||
    lower.includes("not implemented")
  );
}

/** Strip SDK/transport wrappers for settings surfaces. */
export function friendlySettingsError(raw: string, feature: string): string {
  const text = raw.trim();
  if (!text) return `Couldn't update ${feature}.`;
  if (isMissingCapabilityError(text)) {
    return `${feature} isn't available on this workshop yet.`;
  }
  const lower = text.toLowerCase();
  if (lower.includes("versions is off")) {
    return "Turn on vault versioning first.";
  }
  if (
    lower.includes("connection refused") ||
    lower.includes("failed to fetch") ||
    lower.includes("cannot reach") ||
    lower.includes("engine did not become ready") ||
    lower.includes("transport error")
  ) {
    return "Medousa isn't connected right now. Check Connection, then try again.";
  }
  // Prefer the human body after "http error: 404 Not Found: …"
  const stripped = text
    .replace(/^http error:\s*/i, "")
    .replace(/^\d{3}\s+[a-z ]+:\s*/i, "")
    .trim();
  if (
    stripped &&
    stripped.length < 200 &&
    !stripped.toLowerCase().startsWith("http") &&
    !/\b0x[0-9a-f]+\b/i.test(stripped)
  ) {
    return stripped;
  }
  return `Couldn't update ${feature}.`;
}
