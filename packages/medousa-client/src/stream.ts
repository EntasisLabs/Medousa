import type { InteractiveTurnStreamEvent } from "./types.js";

/**
 * A host turn has handed work to the background worker/workshop lane. The
 * daemon keeps the stream alive so a capable surface can observe the later
 * synthesis, but a chat composer should be released at this boundary.
 */
export function isBackgroundHandoffEvent(event: InteractiveTurnStreamEvent): boolean {
  const eventType = event.event_type.toLowerCase();
  const phase = event.phase.toLowerCase();
  return (
    eventType === "worker_ack" ||
    eventType === "workshop_ack" ||
    phase === "worker_ack" ||
    phase === "workshop_ack"
  );
}

export function streamPathWithSince(path: string, since: number): string {
  const url = new URL(path, "http://medousa.invalid");
  if (since > 0) url.searchParams.set("since", String(since));
  return `${url.pathname}${url.search}`;
}

export function parseSseBlock(block: string): InteractiveTurnStreamEvent | null {
  const data = block
    .split(/\r?\n/)
    .filter((line) => line.startsWith("data:"))
    .map((line) => line.slice(5).trimStart())
    .join("\n");
  if (!data || data === "[DONE]") return null;
  const parsed: unknown = JSON.parse(data);
  if (!parsed || typeof parsed !== "object") throw new Error("Invalid Medousa SSE event");
  return parsed as InteractiveTurnStreamEvent;
}

export async function* readSse(
  response: Response,
): AsyncGenerator<InteractiveTurnStreamEvent> {
  if (!response.body) throw new Error("Medousa stream response has no body");
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let pending = "";

  try {
    while (true) {
      const { done, value } = await reader.read();
      pending += decoder.decode(value, { stream: !done });
      const blocks = pending.split(/\r?\n\r?\n/);
      pending = blocks.pop() ?? "";
      for (const block of blocks) {
        const event = parseSseBlock(block);
        if (event) yield event;
      }
      if (done) break;
    }
    if (pending.trim()) {
      const event = parseSseBlock(pending);
      if (event) yield event;
    }
  } finally {
    // A host may intentionally stop at a non-terminal workshop handoff. Release
    // the reader and cancel the underlying request so the foreground stream does
    // not remain open after the composer has been released.
    await reader.cancel().catch(() => undefined);
    reader.releaseLock();
  }
}
