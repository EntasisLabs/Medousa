export class CreativeRequestError extends Error {
  constructor(message: string, readonly status: number | null) { super(message); }
  get rejected() { return this.status !== null && [400, 403, 404, 409, 413, 422].includes(this.status); }
}
export function creativeRequestError(error: unknown) {
  const raw = error instanceof Error ? error.message : String(error);
  const status = /workshop returned HTTP (\d{3})\b/.exec(raw)?.[1];
  return new CreativeRequestError(creativeErrorMessage(error), status ? Number(status) : null);
}
export function creativeErrorText(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  const start = raw.indexOf("{");
  if (start >= 0) {
    try {
      const envelope = JSON.parse(raw.slice(start));
      if (typeof envelope.message === "string") return envelope.message;
    } catch { /* Older transports can return plain text. */ }
  }
  return raw;
}

export function isCreativeAdmissionBusy(error: unknown): boolean {
  const message = creativeErrorText(error).toLowerCase();
  return message.includes("forge queued-byte budget is exhausted") ||
    message.includes("forge command queue is full");
}

export function creativeErrorMessage(error: unknown): string {
  return isCreativeAdmissionBusy(error)
    ? "Your workshop is busy preparing the canvas. Try again in a moment."
    : creativeErrorText(error);
}
