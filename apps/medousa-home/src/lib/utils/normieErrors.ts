/** Map daemon/transport raw errors to operator-facing copy (no jargon). */

export const MAX_MEDIA_REFS_PER_TURN = 5;
export const MAX_MEDIA_UPLOAD_MB = 25;

const GENERIC_TURN =
  "Something went wrong on this turn. Try again in a moment.";

export function friendlyUserError(raw: string, context?: { fileName?: string }): string {
  const trimmed = raw.trim();
  if (!trimmed) return GENERIC_TURN;

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
  if (lower.includes("mime type not allowed") || lower.includes("not allowed")) {
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
    lower.includes("failed to read") ||
    lower.includes("no such file") ||
    (lower.includes("not found") && !lower.includes("model"))
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

export function friendlyTurnError(raw: string): string {
  const text = raw.trim();
  if (!text) return GENERIC_TURN;
  const lower = text.toLowerCase();

  if (lower.includes("cancelled") || lower.includes("canceled")) {
    return "Turn cancelled.";
  }
  if (
    lower.includes("401") ||
    lower.includes("403") ||
    lower.includes("invalid api key") ||
    lower.includes("incorrect api key") ||
    lower.includes("authentication") ||
    lower.includes("unauthorized")
  ) {
    return "The model provider rejected the API key. Check Settings → Models and try again.";
  }
  if (
    lower.includes("429") ||
    lower.includes("rate limit") ||
    lower.includes("too many requests") ||
    lower.includes("quota")
  ) {
    return "The model provider is rate-limiting requests. Wait a moment and try again.";
  }
  if (
    lower.includes("404") ||
    lower.includes("model not found") ||
    lower.includes("does not exist") ||
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
    return "The model took too long to respond. Try again with a shorter message.";
  }
  if (
    lower.includes("connection") ||
    lower.includes("transport") ||
    lower.includes("unavailable") ||
    lower.includes("502") ||
    lower.includes("503") ||
    lower.includes("504") ||
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
    return "Configure a vision model in Settings → Models before sending images.";
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
