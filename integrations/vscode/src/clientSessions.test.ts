import { describe, expect, it } from "vitest";
import { MedousaClient } from "@medousa/client";

describe("Medousa session client", () => {
  it("renames sessions through the daemon-owned name endpoint", async () => {
    let request: { url: string; init?: RequestInit } | undefined;
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input, init) => {
        request = { url: String(input), init };
        return Response.json({ session_id: "session/one", display_name: "Compiler work" });
      },
    });

    await client.renameSession("session/one", "Compiler work");

    expect(request?.url).toBe("http://127.0.0.1:7419/v1/sessions/session%2Fone/name");
    expect(request?.init?.method).toBe("PUT");
    expect(request?.init?.body).toBe(JSON.stringify({ display_name: "Compiler work" }));
  });

  it("requires explicit memory purge intent when deleting a session", async () => {
    let request: { url: string; init?: RequestInit } | undefined;
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input, init) => {
        request = { url: String(input), init };
        return Response.json({ session_id: "session-one", deleted: true });
      },
    });

    await client.deleteSession("session-one", true);

    expect(request?.url).toBe("http://127.0.0.1:7419/v1/sessions/session-one?purge_memory=true");
    expect(request?.init?.method).toBe("DELETE");
  });

  it("saves settled replies through the workshop vault API", async () => {
    let request: { url: string; init?: RequestInit } | undefined;
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input, init) => {
        request = { url: String(input), init };
        return Response.json({ note: { path: "inbox/reply.md" }, created: true });
      },
    });

    await client.createVaultNote({
      path: "inbox/reply.md",
      content: "# Reply",
      session_id: "session-one",
      semantic_tags: ["chat-turn"],
    });

    expect(request?.url).toBe("http://127.0.0.1:7419/v1/vault/notes");
    expect(request?.init?.method).toBe("POST");
    expect(request?.init?.body).toContain('"semantic_tags":["chat-turn"]');
  });
});
