import { setActiveWorkshopIdPort } from "$lib/utils/workshopLocality";
import { subscribeCodeDiagnostics } from "./codeDiagnosticsEvents";
import { invalidateCodeWorkshopContext } from "./codeWorkspaceContext.svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const daemon = vi.hoisted(() => ({
  info: vi.fn(),
  workspace: vi.fn(),
  url: vi.fn(),
  base: vi.fn(),
}));

vi.mock("$lib/daemon", () => ({
  getCodingEngineInfo: daemon.info,
  getGraphemeLspWorkspace: daemon.workspace,
  daemonWebSocketUrl: daemon.url,
  getDaemonUrl: daemon.base,
  OPERATIONS: {
    "code.lsp.get": { path: "/v1/code/lsp" },
    "grapheme.lsp.get": { path: "/v1/grapheme/lsp" },
    "code.language_root.get": { path: "/v1/code/language-root" },
    "code.language_matrix.get": { path: "/v1/code/language-matrix" },
    "code.workspace_diagnostics.get": { path: "/v1/code/workspace-diagnostics" },
  },
}));
vi.mock("$lib/window", () => ({ isTauri: () => false }));

import {
  acquireCodeWorkspaceLspClient,
  connectOrchestratorLspClient,
  getCodeLanguageMatrix,
  getAllCodeWorkspaceDiagnostics,
} from "./codingEngineClient";

/** Simulated wire; initialization, request IDs, and readiness use the real client. */
class LanguageSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static sockets: LanguageSocket[] = [];
  static failInitialize = false;
  static silentInitialize = false;
  static serverName: string | null = null;
  readyState = LanguageSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((event: { code: number; reason: string; wasClean: boolean }) => void) | null = null;
  messages: Array<{ id?: number; method: string; params: Record<string, unknown> }> = [];

  constructor(readonly url: string) {
    LanguageSocket.sockets.push(this);
    queueMicrotask(() => {
      this.readyState = LanguageSocket.OPEN;
      this.onopen?.();
    });
  }

  send(raw: string) {
    const message = JSON.parse(raw);
    this.messages.push(message);
    if (message.method === "initialize" && !LanguageSocket.silentInitialize) {
      queueMicrotask(() => this.onmessage?.({ data: JSON.stringify({
        jsonrpc: "2.0",
        id: message.id,
        ...(LanguageSocket.failInitialize
          ? { error: { code: -32603, message: "language server initialization failed" } }
          : { result: {
              capabilities: { textDocumentSync: 1 },
              ...(LanguageSocket.serverName ? { serverInfo: { name: LanguageSocket.serverName } } : {}),
            } }),
      }) }));
    }
  }

  close(code = 1000, reason = "closed") {
    this.readyState = 3;
    this.onclose?.({ code, reason, wasClean: true });
  }
}

beforeEach(() => {
  vi.clearAllMocks();
  LanguageSocket.sockets = [];
  LanguageSocket.failInitialize = false;
  LanguageSocket.silentInitialize = false;
  LanguageSocket.serverName = null;
  vi.stubGlobal("WebSocket", LanguageSocket);
  daemon.info.mockResolvedValue({
    available: true,
    starting: false,
    daemon_lsp_path: "/v1/code/lsp",
    message: "coding engine reachable",
  });
  daemon.workspace.mockResolvedValue({
    root_uri: "file:///scripts",
    root_path: "/scripts",
    scripts_dir: "/scripts/scripts",
  });
  daemon.url.mockImplementation(async (path: string) => `ws://workshop${path}`);
  daemon.base.mockResolvedValue("http://workshop");
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  setActiveWorkshopIdPort(null);
});

describe("language-service routing", () => {
  it.each(["python", "rust", "typescript", "grapheme"])(
    "reports an unavailable engine without another connection for %s",
    async (language) => {
      daemon.info.mockResolvedValue({
        available: false,
        starting: false,
        message: "incompatible medousa-code is already listening",
      });
      await expect(connectOrchestratorLspClient({ language })).rejects.toThrow(
        "incompatible medousa-code",
      );
      expect(daemon.url).not.toHaveBeenCalled();
      expect(daemon.workspace).not.toHaveBeenCalled();
      expect(LanguageSocket.sockets).toHaveLength(0);
    },
  );

  it.each(["python", "rust", "typescript", "grapheme"])(
    "does not change routes after discovery throws for %s",
    async (language) => {
      daemon.info.mockRejectedValue(new Error("discovery failed"));
      await expect(connectOrchestratorLspClient({ language })).rejects.toThrow("discovery failed");
      expect(daemon.url).not.toHaveBeenCalled();
      expect(LanguageSocket.sockets).toHaveLength(0);
    },
  );

  it.each(["python", "rust", "typescript", "grapheme"])(
    "initializes %s only through the coding engine at its selected root",
    async (language) => {
      const connection = await connectOrchestratorLspClient({
        language,
        workId: "work-1",
        workspaceRoot: "/repo",
        documentUri: "file:///repo/packages/app/source",
        languageRootUri: "file:///repo/packages/app",
      });
      await connection.ready;
      const socket = LanguageSocket.sockets[0]!;
      expect(new URL(socket.url).pathname).toBe("/v1/code/lsp");
      expect(new URL(socket.url).searchParams.get("language")).toBe(language);
      expect(socket.messages[0]).toMatchObject({
        method: "initialize",
        params: { rootUri: "file:///repo/packages/app" },
      });
      expect(socket.messages[1]?.method).toBe("initialized");
      expect(connection.via).toBe("orchestrator");
      connection.client.disconnect();
      connection.close();
    },
  );

  it("keeps a starting engine on its intended route", async () => {
    daemon.info.mockResolvedValue({ available: false, starting: true, daemon_lsp_path: "/v1/code/lsp" });
    const connection = await connectOrchestratorLspClient({ language: "rust" });
    await connection.ready;
    expect(daemon.url).toHaveBeenCalledOnce();
    expect(new URL(LanguageSocket.sockets[0]!.url).pathname).toBe("/v1/code/lsp");
    connection.client.disconnect();
    connection.close();
  });

  it.each([undefined, "/v1/grapheme/lsp"])("rejects an unsupported advertised route: %s", async (path) => {
    daemon.info.mockResolvedValue({ available: true, daemon_lsp_path: path });
    await expect(connectOrchestratorLspClient({ language: "rust" })).rejects.toThrow("does not support");
    expect(daemon.url).not.toHaveBeenCalled();
  });

  it("rejects an implicit language before discovery", async () => {
    await expect(connectOrchestratorLspClient({ language: " " })).rejects.toThrow("explicit language");
    expect(daemon.info).not.toHaveBeenCalled();
  });

  it("closes a failed initialization without trying a second route", async () => {
    LanguageSocket.failInitialize = true;
    const connection = await connectOrchestratorLspClient({ language: "typescript" });
    await expect(connection.ready).rejects.toBeDefined();
    expect(LanguageSocket.sockets).toHaveLength(1);
    expect(LanguageSocket.sockets[0]!.readyState).toBe(3);
    expect(daemon.url).toHaveBeenCalledOnce();
  });

  it("times out initialization on the same service without changing routes", async () => {
    vi.useFakeTimers();
    LanguageSocket.silentInitialize = true;
    const connection = await connectOrchestratorLspClient({ language: "rust" });
    const failure = expect(connection.ready).rejects.toThrow("Request timed out");
    await vi.advanceTimersByTimeAsync(30_001);
    await failure;
    expect(daemon.url).toHaveBeenCalledOnce();
    expect(LanguageSocket.sockets[0]!.readyState).toBe(3);
  });

  it("rejects Grapheme's announced identity for a TypeScript request", async () => {
    LanguageSocket.serverName = "grapheme-lsp";
    const connection = await connectOrchestratorLspClient({ language: "typescript" });
    await expect(connection.ready).rejects.toThrow("Unexpected language server");
    expect(connection.service).toMatchObject({ language: "typescript", serverName: "grapheme-lsp" });
    expect(LanguageSocket.sockets[0]!.readyState).toBe(3);
    expect(LanguageSocket.sockets).toHaveLength(1);
  });

  it("accepts an explicitly selected Grapheme service", async () => {
    LanguageSocket.serverName = "grapheme-lsp";
    const connection = await connectOrchestratorLspClient({ language: "grapheme" });
    await connection.ready;
    expect(connection.service.serverName).toBe("grapheme-lsp");
    connection.client.disconnect();
    connection.close();
  });
});

describe("required language discovery", () => {
  const request = {
    workId: "work-1",
    workspaceRoot: "/repo",
    language: "typescript",
    documentUri: "file:///repo/packages/app/source.ts",
  };

  it("does not open a connection when root discovery fails", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("HTTP 404 Not Found", { status: 404 })));
    await expect(acquireCodeWorkspaceLspClient(request)).rejects.toThrow("HTTP 404");
    expect(daemon.info).not.toHaveBeenCalled();
    expect(LanguageSocket.sockets).toHaveLength(0);
  });

  it.each(["file:///other/package", "https://workshop/repo", "file:///repo/%2foutside"])(
    "does not substitute the project root for invalid root %s",
    async (rootUri) => {
      vi.stubGlobal("fetch", vi.fn(async () => Response.json({
        ok: true, language: "typescript", root_uri: rootUri,
      })));
      await expect(acquireCodeWorkspaceLspClient(request)).rejects.toThrow("invalid language root");
      expect(LanguageSocket.sockets).toHaveLength(0);
    },
  );

  it("rejects a root selected for another language", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Response.json({
      ok: true, language: "grapheme", root_uri: "file:///repo",
    })));
    await expect(acquireCodeWorkspaceLspClient(request)).rejects.toThrow("invalid language root");
    expect(LanguageSocket.sockets).toHaveLength(0);
  });

  it("initializes a discovered nested root and releases its pooled connection", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Response.json({
      ok: true, language: "typescript", root_uri: "file:///repo/packages/app",
    })));
    const lease = await acquireCodeWorkspaceLspClient({ ...request, workId: "nested-root" });
    const client = await lease.client;
    expect(LanguageSocket.sockets[0]!.messages[0]?.params.rootUri).toBe("file:///repo/packages/app");
    expect(client.serverCapabilities).toMatchObject({ textDocumentSync: 1 });
    vi.useFakeTimers();
    lease.release();
    await vi.advanceTimersByTimeAsync(1_001);
    expect(LanguageSocket.sockets[0]!.readyState).toBe(3);
  });

  it("reports pooled startup failure without ready status or an unhandled observer rejection", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Response.json({
      ok: true, language: "typescript", root_uri: "file:///repo/packages/app",
    })));
    daemon.info.mockRejectedValue(new Error("engine discovery failed"));
    const lease = await acquireCodeWorkspaceLspClient({ ...request, workId: "failed-startup" });
    const phases: string[] = [];
    const unsubscribe = lease.subscribeStatus((status) => phases.push(status.phase));
    await expect(lease.client).rejects.toThrow("engine discovery failed");
    expect(phases).toContain("failed");
    expect(phases).not.toContain("ready");
    expect(LanguageSocket.sockets).toHaveLength(0);
    unsubscribe();
    lease.release();
  });

  it("rejects a missing matrix contract rather than an empty success", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Response.json({ ok: true })));
    await expect(getCodeLanguageMatrix()).rejects.toThrow("does not support");
  });

  it("rejects an incomplete matrix rather than inferring usability", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Response.json({
      ok: true, languages: [{ language: "rust", binary_available: true }],
    })));
    await expect(getCodeLanguageMatrix()).rejects.toThrow("does not support");
  });

  it("does not start per-language sessions when aggregate diagnostics are unsupported", async () => {
    const fetch = vi.fn(async () => Response.json({ ok: true, scope: "language", documents: [] }));
    vi.stubGlobal("fetch", fetch);
    await expect(getAllCodeWorkspaceDiagnostics({ workId: "work-1", languages: ["rust"] })).rejects.toThrow("does not support");
    expect(fetch).toHaveBeenCalledOnce();
  });
});

describe("workshop language-service isolation", () => {
  const request = { workId: "identical-id", workspaceRoot: "/repo", language: "typescript", documentUri: "file:///repo/app.ts" };

  it("does not reuse a live client across workshops with identical paths", async () => {
    let workshop = "workshop-a";
    setActiveWorkshopIdPort(() => workshop);
    vi.stubGlobal("fetch", vi.fn(async () => Response.json({ ok: true, language: "typescript", root_uri: "file:///repo" })));
    const first = await acquireCodeWorkspaceLspClient(request);
    const firstClient = await first.client;
    workshop = "workshop-b";
    const second = await acquireCodeWorkspaceLspClient(request);
    expect(await second.client).not.toBe(firstClient);
    expect(LanguageSocket.sockets).toHaveLength(2);
    vi.useFakeTimers();
    first.release();
    await vi.advanceTimersByTimeAsync(1_001);
    expect(LanguageSocket.sockets[0]!.readyState).toBe(3);
    expect(LanguageSocket.sockets[1]!.readyState).toBe(LanguageSocket.OPEN);
    second.release();
    await vi.advanceTimersByTimeAsync(1_001);
  });

  it("does not connect after returning from another workshop during root discovery", async () => {
    let resolve!: (response: Response) => void;
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>((done) => { resolve = done; })));
    const pending = acquireCodeWorkspaceLspClient(request);
    await vi.waitFor(() => expect(resolve).toBeDefined());
    invalidateCodeWorkshopContext();
    resolve(Response.json({ ok: true, language: "typescript", root_uri: "file:///repo" }));
    await expect(pending).rejects.toThrow("workshop changed");
    expect(daemon.info).not.toHaveBeenCalled();
    expect(LanguageSocket.sockets).toHaveLength(0);
  });

  it("does not open a socket after engine discovery finishes on a different workshop", async () => {
    let workshop = "workshop-a";
    setActiveWorkshopIdPort(() => workshop);
    let resolve!: (info: unknown) => void;
    daemon.info.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const pending = connectOrchestratorLspClient({ language: "rust" });
    workshop = "workshop-b";
    resolve({ available: true, daemon_lsp_path: "/v1/code/lsp" });
    await expect(pending).rejects.toThrow("workshop changed");
    expect(LanguageSocket.sockets).toHaveLength(0);
  });
});

it("invalidates project observations on wire diagnostics without crossing workshop or project scope", async () => {
  let workshop = "workshop-a";
  setActiveWorkshopIdPort(() => workshop);
  const same = vi.fn();
  const other = vi.fn();
  const stopSame = subscribeCodeDiagnostics("project", same);
  const stopOther = subscribeCodeDiagnostics("other-project", other);
  const connection = await connectOrchestratorLspClient({ language: "rust", workId: "project", workspaceRoot: "/repo", languageRootUri: "file:///repo" });
  await connection.ready;
  const socket = LanguageSocket.sockets[0];
  const publish = () => socket.onmessage?.({ data: JSON.stringify({ jsonrpc: "2.0", method: "textDocument/publishDiagnostics", params: { uri: "file:///repo/other.rs", diagnostics: [] } }) });
  publish();
  expect(same).toHaveBeenCalledTimes(1);
  expect(other).not.toHaveBeenCalled();
  workshop = "workshop-b";
  const newWorkshop = vi.fn();
  const stopNew = subscribeCodeDiagnostics("project", newWorkshop);
  publish();
  expect(same).toHaveBeenCalledTimes(1);
  expect(newWorkshop).not.toHaveBeenCalled();
  stopSame(); stopOther(); stopNew();
  connection.close();
});
