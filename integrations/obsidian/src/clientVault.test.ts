import { describe, expect, it } from "vitest";
import { MedousaClient } from "@medousa/client";

describe("Medousa vault client", () => {
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
