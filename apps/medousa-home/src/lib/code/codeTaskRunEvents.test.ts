import { describe, expect, it } from "vitest";

import {
  CodeTaskRunEventStream,
  parseTaskRunEventPayload,
  taskRunEventIsTerminal,
} from "$lib/code/codeTaskRunEvents";
import { streamPathWithSince } from "$lib/stream/reconnect";

describe("code task run events", () => {
  it("builds resumable task-run events paths with since", () => {
    expect(
      streamPathWithSince("/v1/forge/items/w1/task-runs/run-1/events", 0),
    ).toBe("/v1/forge/items/w1/task-runs/run-1/events");
    expect(
      streamPathWithSince("/v1/forge/items/w1/task-runs/run-1/events", 9),
    ).toBe("/v1/forge/items/w1/task-runs/run-1/events?since=9");
  });

  it("parses task output payloads", () => {
    expect(
      parseTaskRunEventPayload(
        JSON.stringify({
          seq: 2,
          run_id: "run-1",
          kind: "output",
          stream: "stdout",
          text: "ok\n",
        }),
      ),
    ).toMatchObject({ seq: 2, kind: "output", text: "ok\n" });
    expect(parseTaskRunEventPayload("{")).toBeNull();
    expect(parseTaskRunEventPayload(JSON.stringify({ seq: 1 }))).toBeNull();
  });

  it("treats only state+result as terminal", () => {
    expect(
      taskRunEventIsTerminal({
        seq: 1,
        run_id: "run-1",
        kind: "state",
        state: "cancelled",
      }),
    ).toBe(false);
    expect(
      taskRunEventIsTerminal({
        seq: 2,
        run_id: "run-1",
        kind: "state",
        state: "passed",
        result: {
          task: { id: "t", label: "T", kind: "test", argv: [], provider: "x" },
          success: true,
          stdout: "",
          stderr: "",
          truncated: false,
          duration_ms: 1,
          locations: [],
        },
      }),
    ).toBe(true);
  });

  it("treats since as the next sequence so event zero is not skipped", () => {
    const stream = new CodeTaskRunEventStream({ onEvent: () => {} });
    stream.start("work-1", "run-1", 0);
    expect(stream.cursor).toBe(0);
    stream.start("work-1", "run-1", 9);
    expect(stream.cursor).toBe(9);
    stream.teardown();
  });
});
