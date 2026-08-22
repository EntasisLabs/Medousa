import { describe, expect, it } from "vitest";
import {
  CODE_LSP_MAX_RECONNECT_ATTEMPTS,
  codeLanguageServerEventFromMessage,
  codeLspReconnectDelay,
  codeWorkspaceLspPoolKey,
  smartEditingUnavailableMessage,
} from "./codingEngineClient";

describe("coding-engine workspace client identity", () => {
  it("reuses a project-language client for equivalent root URIs", () => {
    expect(
      codeWorkspaceLspPoolKey(
        "work-1",
        "TypeScript",
        "FILE://LOCALHOST/repo/packages/app",
      ),
    ).toBe(
      codeWorkspaceLspPoolKey(
        "work-1",
        "typescript",
        "file:///repo/packages/app",
      ),
    );
  });

  it("keeps nested language roots and governed projects isolated", () => {
    const app = codeWorkspaceLspPoolKey(
      "work-1",
      "typescript",
      "file:///repo/packages/app",
    );
    expect(
      codeWorkspaceLspPoolKey(
        "work-1",
        "typescript",
        "file:///repo/packages/api",
      ),
    ).not.toBe(app);
    expect(
      codeWorkspaceLspPoolKey(
        "work-2",
        "typescript",
        "file:///repo/packages/app",
      ),
    ).not.toBe(app);
  });
});

describe("coding-engine lifecycle", () => {
  it("uses a bounded reconnect schedule", () => {
    expect(codeLspReconnectDelay(1)).toBe(250);
    expect(codeLspReconnectDelay(2)).toBe(750);
    expect(codeLspReconnectDelay(CODE_LSP_MAX_RECONNECT_ATTEMPTS)).toBe(1_500);
    expect(codeLspReconnectDelay(CODE_LSP_MAX_RECONNECT_ATTEMPTS + 1)).toBeNull();
  });

  it("normalizes work-done progress and server log notifications", () => {
    expect(
      codeLanguageServerEventFromMessage(
        JSON.stringify({
          jsonrpc: "2.0",
          method: "$/progress",
          params: {
            token: "index",
            value: {
              kind: "report",
              message: "Indexing",
              percentage: 120,
            },
          },
        }),
      ),
    ).toEqual({
      kind: "progress",
      token: "index",
      progressKind: "report",
      title: "",
      message: "Indexing",
      percentage: 100,
    });
    expect(
      codeLanguageServerEventFromMessage(
        JSON.stringify({
          jsonrpc: "2.0",
          method: "window/logMessage",
          params: { type: 2, message: "Project reload required" },
        }),
      ),
    ).toEqual({
      kind: "log",
      level: "warning",
      message: "Project reload required",
    });
  });

  it("turns protected websocket failures into an actionable workshop repair", () => {
    expect(smartEditingUnavailableMessage("rust", "HTTP 401 Unauthorized")).toContain(
      "Settings → Connection",
    );
    expect(smartEditingUnavailableMessage("rust", "rust-analyzer exited")).toBe(
      "Smart editing is unavailable for rust: rust-analyzer exited",
    );
  });
});

describe("coding-engine language matrix helpers", () => {
  it("finds matrix rows by language id", async () => {
    const { findCodeLanguageMatrixEntry } = await import("./codingEngineClient");
    const row = findCodeLanguageMatrixEntry(
      [
        {
          language: "Svelte",
          command: "svelteserver",
          binaryAvailable: false,
          usable: false,
          packageId: "langservers",
          rootMarkers: ["svelte.config.js"],
          extensions: ["svelte"],
          args: ["--stdio"],
        },
      ],
      "svelte",
    );
    expect(row?.packageId).toBe("langservers");
    expect(row?.usable).toBe(false);
  });
});
