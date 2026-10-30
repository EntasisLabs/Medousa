import { getWorkspaceCard } from "$lib/daemon";
import { setActiveWorkshopIdPort } from "$lib/utils/workshopLocality";
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
    execution_runtime_id: "runtime-mac-mini",
    ...overrides,
  };
}

describe("WorkerTranscriptStore", () => {
  let store: WorkerTranscriptStore;

  beforeEach(() => {
    vi.clearAllMocks();
    setActiveWorkshopIdPort(null);
    store = new WorkerTranscriptStore();
  });

  it("ingests live tool activity, thinking, and output from card detail", () => {
    const transcript = store.ingestDetail(detail(), "in_flight");
    expect(transcript).not.toBeNull();
    expect(transcript?.workId).toBe("work-1");
    expect(transcript?.title).toBe("Research the thing");
    expect(transcript?.disposition).toBe("parallel");
    expect(transcript?.model).toBe("gpt-4o");
    expect(transcript?.executionRuntimeId).toBe("runtime-mac-mini");
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
        execution_runtime_id: "runtime-studio",
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
    expect(transcript?.executionRuntimeId).toBe("runtime-studio");
    expect(transcript?.toolRuns[0]?.input_params?.[0]?.value).toBe("svelte 5 runes");
    expect(transcript?.streaming).toBe(true);
  });

  it.each(['backlog', 'wrapping_up', 'blocked'])("does not stream non-executing %s detail or progress", column => {
    const fromDetail = store.ingestDetail(detail({ terminal: false }), column)!;
    expect(fromDetail.streaming).toBe(false);
    expect(fromDetail.terminal).toBe(false);
    const fromProgress = store.ingestProgress({ work_id: 'work-2', column, terminal: false, live_status_line: 'Old tool status' })!;
    expect(fromProgress.streaming).toBe(false);
    expect(fromProgress.terminal).toBe(false);
    if (column === 'wrapping_up') {
      expect(fromDetail.statusLine).toBe('Result ready · Awaiting summary');
      expect(fromProgress.statusLine).toBe('Result ready · Awaiting summary');
    }
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
  it("loads full persisted text and follow-ups without subsequent previews truncating it", async () => {
    const full = detail({ worker_transcript: {
      work_id: "work-1", session_id: "sess-1", parent_runtime_id: "host",
      execution_runtime_id: "runtime-mac-mini", task_prompt: "prompt ".repeat(300),
      result_text: "result ".repeat(300), steer_messages: [{ text: "Follow up", at: "now" }],
      retention_note: "Recorded evidence only",
    }});
    vi.mocked(getWorkspaceCard).mockResolvedValue(full);
    store.ingestProgress({ work_id: "work-1", column: "done", terminal: true, result_excerpt: "short" });
    const loaded = await store.refresh("work-1");
    expect(getWorkspaceCard).toHaveBeenCalledOnce();
    expect(loaded?.taskPrompt).toBe(full.worker_transcript!.task_prompt);
    expect(loaded?.resultText).toBe(full.worker_transcript!.result_text);
    expect(loaded?.steerMessages?.[0].text).toBe("Follow up");
    expect(loaded?.toolRuns).toHaveLength(1);
    store.ingestProgress({ work_id: "work-1", column: "done", terminal: true, result_excerpt: "short" });
    store.ingestDetail(detail({ result_excerpt: "short" }), "done");
    expect(store.transcriptFor("work-1")?.resultText).toBe(full.worker_transcript!.result_text);
  });

  it('does not promote an excerpt when the persisted detail has no final result', () => {
    store.ingestDetail(detail({ result_excerpt: 'Short preview' }), 'in_flight');
    store.ingestDetail(detail({ worker_transcript: {
      work_id: 'work-1', session_id: 'sess-1', parent_runtime_id: 'host',
      execution_runtime_id: 'runtime-mac-mini', task_prompt: 'Full assignment', result_text: null,
      steer_messages: [], retention_note: 'Rolling output only',
    } }), 'in_flight');
    expect(store.transcriptFor('work-1')?.historyLoaded).toBe(true);
    expect(store.transcriptFor('work-1')?.resultText).toBeNull();
  });

  it("does not hydrate foreign work or resurrect a cleared request", async () => {
    vi.mocked(getWorkspaceCard).mockResolvedValue(detail({ work_id: "work-other" }));
    expect(await store.refresh("work-1")).toBeNull();
    let resolve!: (value: WorkCardDetail) => void;
    vi.mocked(getWorkspaceCard).mockReturnValue(new Promise(done => { resolve = done; }));
    const pending = store.refresh("work-1");
    store.clear("work-1");
    resolve(detail());
    expect(await pending).toBeNull();
    expect(store.transcriptFor("work-1")).toBeNull();
  });

  it("drops responses and cache across workshop authority changes", async () => {
    let scope = "one";
    setActiveWorkshopIdPort(() => scope);
    store.ingestDetail(detail(), "in_flight");
    let resolve!: (value: WorkCardDetail) => void;
    vi.mocked(getWorkspaceCard).mockReturnValue(new Promise(done => { resolve = done; }));
    const pending = store.refresh("work-1");
    scope = "two";
    resolve(detail());
    expect(await pending).toBeNull();
    expect(store.transcriptFor("work-1")).toBeNull();
    setActiveWorkshopIdPort(null);
  });

  it("keeps observed tools when rolling progress advances and updates matched runs", () => {
    const first = detail();
    store.ingestDetail(first, "in_flight");
    const nextRun = { ...first.live_tool_activity![0], run_id: "run-2", status: "running" };
    store.ingestProgress({ work_id: "work-1", column: "in_flight", terminal: false, live_tool_activity: [nextRun] });
    store.ingestProgress({ work_id: "work-1", column: "in_flight", terminal: false, live_tool_activity: [{...nextRun, status: "succeeded"}] });
    store.ingestProgress({ work_id: "work-1", column: "in_flight", terminal: false, live_tool_activity: [nextRun] });
    expect(store.transcriptFor("work-1")?.toolRuns.map(run => run.status)).toEqual(["running", "succeeded"]);
  });

});
