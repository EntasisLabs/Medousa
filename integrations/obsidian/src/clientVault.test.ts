import { describe, expect, it } from "vitest";
import { MedousaClient } from "@medousa/client";

describe("Medousa vault client", () => {
  it("stops the foreground stream at a workshop handoff", async () => {
    let streamRequests = 0;
    const handoff = {
      schema_version: 2,
      turn_id: "turn-1",
      seq: 1,
      emitted_at_utc: "now",
      event: {
        type: "worker_ack",
        ack_kind: "workshop",
        text: "I’m taking this into the workshop.",
      },
    };
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (_input, init) => {
        if (init?.signal) expect(init.signal.aborted).toBe(false);
        expect((init?.headers as Record<string, string>).Accept).toBe(
          "text/event-stream; medousa-version=2",
        );
        streamRequests += 1;
        const body = new ReadableStream<Uint8Array>({
          start(controller) {
            controller.enqueue(new TextEncoder().encode(`data: ${JSON.stringify(handoff)}\n\n`));
            controller.close();
          },
        });
        return new Response(body, { headers: { "Content-Type": "text/event-stream" } });
      },
    });

    const events = [];
    for await (const event of client.streamTurnV2({
      accepted_at_utc: "now",
      fallback_to_local: false,
      stream_ready: true,
      stream_url: "/v1/interactive/turns/turn-1/stream",
      turn_id: "turn-1",
    }, { stopOnHandoff: true })) {
      events.push(event);
    }

    expect(events).toHaveLength(1);
    expect(events[0]?.event.type).toBe("worker_ack");
    expect(streamRequests).toBe(1);
  });

  it("reconnects the v2 stream from the last sequence and drops replay overlap", async () => {
    const requests: string[] = [];
    const envelopes = [
      {
        schema_version: 2,
        turn_id: "turn-1",
        seq: 1,
        emitted_at_utc: "now",
        event: { type: "content_append", text: "Hel" },
      },
      {
        schema_version: 2,
        turn_id: "turn-1",
        seq: 2,
        emitted_at_utc: "now",
        event: { type: "final", text: "Hello", finish_reason: "complete" },
      },
    ];
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input) => {
        const requestUrl = String(input);
        requests.push(requestUrl);
        const replay = requests.length === 1 ? [envelopes[0]] : envelopes;
        const body = new ReadableStream<Uint8Array>({
          start(controller) {
            for (const envelope of replay) {
              controller.enqueue(
                new TextEncoder().encode(`data: ${JSON.stringify(envelope)}\n\n`),
              );
            }
            controller.close();
          },
        });
        return new Response(body, { headers: { "Content-Type": "text/event-stream" } });
      },
    });

    const sequences: number[] = [];
    for await (const envelope of client.streamTurnV2(
      {
        accepted_at_utc: "now",
        fallback_to_local: false,
        stream_ready: true,
        stream_url: "/v1/interactive/turns/turn-1/stream",
        turn_id: "turn-1",
      },
      { reconnectDelayMs: () => 0 },
    )) {
      sequences.push(envelope.seq);
    }

    expect(sequences).toEqual([1, 2]);
    expect(requests).toHaveLength(2);
    expect(new URL(requests[1] ?? "").searchParams.get("since")).toBe("1");
  });

  it("preserves the host receiver required by browser fetch", async () => {
    let receiver: unknown;
    const hostFetch = function (this: unknown, _input: RequestInfo | URL, _init?: RequestInit): Promise<Response> {
      receiver = this;
      return Promise.resolve(Response.json({ ok: true }));
    } as typeof globalThis.fetch;
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: hostFetch,
    });

    await client.health();

    expect(receiver).toBe(globalThis);
  });

  it("reads a nested note with path-safe URL segments", async () => {
    let request: { url: string; init?: RequestInit } | undefined;
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input, init) => {
        request = { url: String(input), init };
        return Response.json({ note: {}, content: "# Inbox" });
      },
    });

    await client.getVaultNote("inbox/Research notes.md");

    expect(request?.url).toBe("http://127.0.0.1:7419/v1/vault/notes/inbox/Research%20notes.md");
    expect(request?.init?.method).toBeUndefined();
  });

  it("updates note content with an optimistic concurrency hash", async () => {
    let request: { url: string; init?: RequestInit } | undefined;
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input, init) => {
        request = { url: String(input), init };
        return Response.json({ note: {}, created: false });
      },
    });

    await client.updateVaultNote("daily.md", "# Updated\n", "sha256:before");

    const headers = request?.init?.headers as Record<string, string>;
    expect(request?.url).toBe("http://127.0.0.1:7419/v1/vault/notes/daily.md");
    expect(request?.init?.method).toBe("PUT");
    expect(request?.init?.body).toBe("# Updated\n");
    expect(headers["Content-Type"]).toBe("text/markdown; charset=utf-8");
    expect(headers["If-Match"]).toBe("sha256:before");
  });

  it("encodes search and backlink query parameters", async () => {
    const requests: string[] = [];
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input) => {
        requests.push(String(input));
        return Response.json({ hits: [], backlinks: [] });
      },
    });

    await client.searchVault("graph theory", 7);
    await client.vaultBacklinks("journal/Today & tomorrow.md");

    const searchRequest = requests[0];
    const backlinksRequest = requests[1];
    if (!searchRequest || !backlinksRequest) throw new Error("expected both vault requests");
    const search = new URL(searchRequest);
    const backlinks = new URL(backlinksRequest);
    expect(search.pathname).toBe("/v1/vault/search");
    expect(search.searchParams.get("q")).toBe("graph theory");
    expect(search.searchParams.get("limit")).toBe("7");
    expect(backlinks.pathname).toBe("/v1/vault/backlinks");
    expect(backlinks.searchParams.get("path")).toBe("journal/Today & tomorrow.md");
  });
});
