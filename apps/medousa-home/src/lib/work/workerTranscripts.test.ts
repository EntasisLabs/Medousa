import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkCardDetail } from "$lib/types/card";
import { WorkerTranscriptStore } from "$lib/work/workerTranscripts.svelte";

vi.mock("$lib/daemon", () => ({
  getWorkspaceCard: vi.fn(),
}));

function detail(overrides: Partial<WorkCardDetail> = {}): WorkCardDetail {
  return {
    card: {
      id: "work-1",
      column: "in_flight",
      title: "Research the thing",
      status_label: "running",
      created_at_utc: "2026-08-07T00:00:00Z",
      updated_at_utc: "2026-08-07T00:00:00Z",
    },
    kind: "turn_worker",
    subtitle: "research",
    session_id: "sess-1",
    correlation_id: "turn-1",
    work_id: "work-1",
    user_ack: "Research the thing",
    wrapping_up_reasons: [],
    terminal: false,
    task_line: "Research the thing",
    tool_names: ["cognition_grapheme_run"],
    associations: { vault_paths: [], artifact_ids: [], locus_node_ids: [] },
    live_tool_activity: [
      {
        run_id: "run-1",
        name: "cognition_grapheme_run",
        round: 1,
        status: "running",
        input_summary: "scan repo",
        input_params: [{ key: "query", value: "entry point", truncated: false }],
        started_at: "2026-08-07T00:00:01Z",
      },
    ],
    live_thinking: "Considering the approach…",
    live_output: "Found the entry point.",
    thinking_started_at: "2026-08-07T00:00:01Z",
    thinking_finished_at: "2026-08-07T00:00:04Z",
    live_status_line: "Running cognition_grapheme_run…",
    model: "gpt-4o",
    ...overrides,
  };
}

describe("WorkerTranscriptStore", () => {
  let store: WorkerTranscriptStore;

  beforeEach(() => {
    store = new WorkerTranscriptStore();
  });

  it("ingests live tool activity, thinking, and output from card detail", () => {
    const transcript = store.ingestDetail(detail(), "in_flight");
    expect(transcript).not.toBeNull();
    expect(transcript?.workId).toBe("work-1");
    expect(transcript?.title).toBe("Research the thing");
    expect(transcript?.disposition).toBe("parallel");
    expect(transcript?.model).toBe("gpt-4o");
    expect(transcript?.toolRuns).toHaveLength(1);
    expect(transcript?.toolRuns[0]?.name).toBe("cognition_grapheme_run");
    expect(transcript?.toolRuns[0]?.status).toBe("running");
    expect(transcript?.toolRuns[0]?.input_params?.[0]?.value).toBe("entry point");
    expect(transcript?.thinking).toBe("Considering the approach…");
    expect(transcript?.output).toBe("Found the entry point.");
    expect(transcript?.streaming).toBe(true);
    expect(transcript?.terminal).toBe(false);
  });

  it("ingests a pushed progress frame without a card detail fetch", () => {
    const transcript = store.ingestProgress(
      {
        work_id: "work-2",
        session_id: "sess-1",
        live_tool_activity: [
          {
            run_id: "run-9",
            name: "web_search",
            round: 1,
            status: "succeeded",
            input_params: [{ key: "query", value: "svelte 5 runes", truncated: false }],
            output_summary: "6 results",
            started_at: "2026-08-07T00:00:01Z",
            finished_at: "2026-08-07T00:00:03Z",
          },
        ],
        live_thinking: "Searching for docs…",
        live_status_line: "Ran web_search",
        terminal: false,
        column: "in_flight",
      },
      "Look up runes",
    );
    expect(transcript?.workId).toBe("work-2");
    expect(transcript?.title).toBe("Look up runes");
    expect(transcript?.toolRuns[0]?.input_params?.[0]?.value).toBe("svelte 5 runes");
    expect(transcript?.streaming).toBe(true);
  });

  it("marks terminal when column is done", () => {
    const transcript = store.ingestDetail(
      detail({
        card: { ...detail().card, column: "done" },
        terminal: true,
        live_status_line: null,
        result_excerpt: "Here is the report.",
      }),
      "done",
    );
    expect(transcript?.terminal).toBe(true);
    expect(transcript?.streaming).toBe(false);
    expect(transcript?.resultText).toBe("Here is the report.");
  });

  it("detects bound disposition from workshop subtitle", () => {
    const transcript = store.ingestDetail(
      detail({ subtitle: "bound workshop" }),
      "in_flight",
    );
    expect(transcript?.disposition).toBe("bound");
  });

  it("keeps live transcript across non-forced refresh once terminal", async () => {
    store.ingestDetail(
      detail({
        card: { ...detail().card, column: "done" },
        terminal: true,
      }),
      "done",
    );
    const before = store.transcriptFor("work-1");
    const after = await store.refresh("work-1");
    expect(after?.workId).toBe(before?.workId);
  });
});
