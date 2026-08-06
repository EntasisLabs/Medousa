import { describe, expect, it } from "vitest";

import {
  parseProjectEventPayload,
  planOpenBufferAction,
  watchedFileChangesForProjectEvent,
  type ForgeProjectEvent,
} from "$lib/code/codeProjectEvents";
import { streamPathWithSince } from "$lib/stream/reconnect";

function event(
  partial: Partial<ForgeProjectEvent> & Pick<ForgeProjectEvent, "kind">,
): ForgeProjectEvent {
  return {
    seq: partial.seq ?? 1,
    work_id: partial.work_id ?? "work-1",
    kind: partial.kind,
    path: partial.path,
    old_path: partial.old_path,
    digest: partial.digest,
    updated_at: partial.updated_at ?? "2026-01-01T00:00:00Z",
  };
}

describe("code project events", () => {
  it("builds resumable project-events paths with since", () => {
    expect(
      streamPathWithSince("/v1/forge/items/w1/project-events", 0),
    ).toBe("/v1/forge/items/w1/project-events");
    expect(
      streamPathWithSince("/v1/forge/items/w1/project-events", 17),
    ).toBe("/v1/forge/items/w1/project-events?since=17");
  });

  it("plans open-buffer actions from project event kinds", () => {
    expect(planOpenBufferAction(event({ kind: "changed", path: "a.ts" }))).toEqual({
      action: "reconcile",
      path: "a.ts",
    });
    expect(
      planOpenBufferAction(
        event({ kind: "renamed", old_path: "a.ts", path: "b.ts" }),
      ),
    ).toEqual({ action: "rename", oldPath: "a.ts", newPath: "b.ts" });
    expect(planOpenBufferAction(event({ kind: "deleted", path: "a.ts" }))).toEqual({
      action: "delete",
      path: "a.ts",
    });
    expect(planOpenBufferAction(event({ kind: "snapshot" }))).toEqual({
      action: "reconcile_all",
    });
    expect(planOpenBufferAction(event({ kind: "changed" }))).toEqual({
      action: "ignore",
    });
  });

  it("maps project events to didChangeWatchedFiles payloads", () => {
    const toUri = (path: string) => `file:///repo/${path}`;
    expect(
      watchedFileChangesForProjectEvent(
        event({ kind: "created", path: "new.ts" }),
        toUri,
      ),
    ).toEqual([{ uri: "file:///repo/new.ts", type: 1 }]);
    expect(
      watchedFileChangesForProjectEvent(
        event({ kind: "changed", path: "a.ts" }),
        toUri,
      ),
    ).toEqual([{ uri: "file:///repo/a.ts", type: 2 }]);
    expect(
      watchedFileChangesForProjectEvent(
        event({ kind: "deleted", path: "gone.ts" }),
        toUri,
      ),
    ).toEqual([{ uri: "file:///repo/gone.ts", type: 3 }]);
    expect(
      watchedFileChangesForProjectEvent(
        event({ kind: "renamed", old_path: "old.ts", path: "new.ts" }),
        toUri,
      ),
    ).toEqual([
      { uri: "file:///repo/old.ts", type: 3 },
      { uri: "file:///repo/new.ts", type: 1 },
    ]);
  });

  it("parses project SSE payloads and rejects junk", () => {
    expect(
      parseProjectEventPayload(
        JSON.stringify(event({ kind: "changed", path: "a.ts", seq: 9 })),
      )?.seq,
    ).toBe(9);
    expect(parseProjectEventPayload("{")).toBeNull();
    expect(parseProjectEventPayload(JSON.stringify({ seq: 1 }))).toBeNull();
  });
});
