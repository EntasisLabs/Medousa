import type { InteractiveTurnStreamEvent } from "./types.js";

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
    reader.releaseLock();
  }
}
