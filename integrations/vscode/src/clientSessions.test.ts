import { describe, expect, it } from "vitest";
import { MedousaClient, MedousaCompatibilityError } from "@medousa/client";

function healthPayload(contractRevision = 1) {
  return {
    runtime: {
      authority_id: `auth_${"a".repeat(64)}`,
      product_version: "0.9.1",
      build_revision: "test-build-42",
      contract_revision: contractRevision,
      base_schema_revision: 1,
      deployment_profile: "full",
      deployment_target: "full:macos:aarch64",
      advertised_capabilities: ["transport.http"],
    },
    status: "ok",
    backend: "test",
    worker_id: "worker-1",
    now_utc: "2026-01-01T00:00:00Z",
  };
}

describe("Medousa session client", () => {
  it("uses protected health and rejects an incompatible responder", async () => {
    const requests: string[] = [];
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input) => {
        requests.push(String(input));
        return Response.json(healthPayload(2));
      },
    });

    await expect(client.health()).rejects.toBeInstanceOf(MedousaCompatibilityError);
    expect(requests).toEqual(["http://127.0.0.1:7419/v1/health"]);
  });

  it("keeps workshop authority required for session responses", async () => {
    const requests: string[] = [];
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input) => {
        const url = String(input);
        requests.push(url);
        if (url.endsWith("/v1/health")) return Response.json(healthPayload());
        return Response.json({ session_id: "session-one", catalog: "single" });
      },
    });

    await expect(client.createSession()).rejects.toThrow("test-build-42");
    expect(requests).toEqual([
      "http://127.0.0.1:7419/v1/sessions",
      "http://127.0.0.1:7419/v1/health",
    ]);
  });

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

  it("shares mode and Forge binding through daemon-owned session endpoints", async () => {
    const requests: Array<{ url: string; init?: RequestInit }> = [];
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input, init) => {
        requests.push({ url: String(input), init });
        return Response.json({
          session_id: "session/one",
          effective_mode: "coder",
          effective_source: "session",
          revision: 2,
          work_id: "work-1",
        });
      },
    });

    await client.setSessionCodeBinding("session/one", "work-1");
    await client.setSessionAgentMode("session/one", "coder");
    await client.decideAgentModeProposal("session/one", "proposal/one", true);

    expect(requests.map((request) => request.url)).toEqual([
      "http://127.0.0.1:7419/v1/sessions/session%2Fone/code-binding",
      "http://127.0.0.1:7419/v1/sessions/session%2Fone/agent-mode",
      "http://127.0.0.1:7419/v1/sessions/session%2Fone/agent-mode/proposals/proposal%2Fone",
    ]);
    expect(requests[0]?.init?.body).toBe(JSON.stringify({ work_id: "work-1" }));
    expect(requests[1]?.init?.body).toBe(JSON.stringify({
      mode: "coder",
      scope: "session",
    }));
    expect(requests[2]?.init?.body).toBe(JSON.stringify({ accept: true }));
  });

  it("creates and binds a blank project through one session operation", async () => {
    let request: { url: string; init?: RequestInit } | undefined;
    const client = new MedousaClient({
      baseUrl: "http://127.0.0.1:7419",
      fetch: async (input, init) => {
        request = { url: String(input), init };
        return Response.json({ session_id: "session-one", work_id: "work-1" });
      },
    });

    await client.startSessionCodeProject("session-one", {
      title: "Finance dashboard",
      brief: "Track monthly cash flow",
      source: "blank",
    });

    expect(request?.url).toBe("http://127.0.0.1:7419/v1/sessions/session-one/code-project");
    expect(request?.init?.method).toBe("POST");
    expect(request?.init?.body).toBe(JSON.stringify({
      title: "Finance dashboard",
      brief: "Track monthly cash flow",
      source: "blank",
    }));
  });
});
