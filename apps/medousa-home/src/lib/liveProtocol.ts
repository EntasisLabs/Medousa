import type { LiveTranscriptEntry } from "$lib/liveVoice";
import type { LiveWorkResult } from "$lib/liveWorkResult";

export interface LiveFragment extends LiveTranscriptEntry {
  startMs: number;
  endMs: number;
}

/** Keep original fragments; caption grouping is never an execution boundary. */
export class LiveTimeline {
  private fragments: LiveFragment[] = [];
  private seen = new Set<string>();

  accept(event: Record<string, unknown>): boolean {
    const role = event.type === "session.input_transcript.delta" ? "user"
      : event.type === "session.output_transcript.delta" ? "assistant" : null;
    if (!role || typeof event.delta !== "string" || !event.delta
      || typeof event.start_ms !== "number" || typeof event.end_ms !== "number"
      || !Number.isFinite(event.start_ms) || !Number.isFinite(event.end_ms)
      || event.start_ms < 0 || event.end_ms < event.start_ms) return false;
    const id = typeof event.event_id === "string" ? event.event_id
      : `${role}-${event.start_ms}-${event.end_ms}-${event.delta}`;
    if (this.seen.has(id)) return false;
    this.seen.add(id);
    this.fragments.push({ id, role, text: event.delta, startMs: event.start_ms, endMs: event.end_ms });
    return true;
  }

  snapshot(offsetMs = Infinity): LiveTranscriptEntry[] {
    const rows: Array<LiveTranscriptEntry & { endMs: number }> = [];
    const last = new Map<string, LiveTranscriptEntry & { endMs: number }>();
    for (const fragment of [...this.fragments].sort((a, b) => a.startMs - b.startMs)) {
      if (fragment.startMs > offsetMs) continue;
      const previous = last.get(fragment.role);
      // Heuristic display rows, independently grouped for overlapping speakers.
      if (previous && fragment.startMs - previous.endMs <= 1200) {
        previous.text += fragment.text;
        previous.endMs = Math.max(previous.endMs, fragment.endMs);
      } else {
        const row = { ...fragment };
        rows.push(row);
        last.set(fragment.role, row);
      }
    }
    return rows.map(({ id, role, text }) => ({ id, role, text }));
  }

  requestBetween(previousOffset: number, offset: number): string {
    if (previousOffset < 0) {
      return this.snapshot(offset).filter((row) => row.role === "user").at(-1)?.text.trim() ?? "";
    }
    return [...this.fragments].sort((a, b) => a.startMs - b.startMs)
      .filter((fragment) => fragment.role === "user" && fragment.endMs > previousOffset && fragment.startMs <= offset)
      .map((fragment) => fragment.text).join("").trim();
  }
}

export function liveDelegation(event: Record<string, unknown>): { id: string; offsetMs: number } | null {
  if (event.type !== "session.delegation.created") return null;
  const delegation = event.delegation as Record<string, unknown> | undefined;
  if (!delegation || delegation.target !== "client" || typeof delegation.id !== "string"
    || !delegation.id || typeof event.offset_ms !== "number" || !Number.isFinite(event.offset_ms)) return null;
  return { id: delegation.id, offsetMs: event.offset_ms };
}

export function liveDelegationResult(id: string, result: LiveWorkResult) {
  // Each append must stay below 500 tokens, even for multibyte text. Keep full
  // output in the daemon; never try to read a 12KB tool payload aloud.
  let excerpt = "";
  let bytes = 0;
  for (const character of result.text) {
    const size = new TextEncoder().encode(character).length;
    if (bytes + size > 360) break;
    excerpt += character;
    bytes += size;
  }
  const truncated = excerpt.length < result.text.length;
  return {
    type: "session.commentary.append",
    event_id: `result-${id}`,
    delegation_id: id,
    content: `${result.status === "completed" ? "" : `${result.status}: `}${excerpt}${truncated ? "… Full result is in the chat." : ""}`,
  };
}
