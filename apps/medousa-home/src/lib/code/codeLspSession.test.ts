import { afterEach, describe, expect, it, vi } from "vitest";
import type { CodeWorkspaceLspLease, CodeWorkspaceLspStatus } from "./codingEngineClient";
import {
  CodeLspSession,
  planLspReconnect,
  unusableLanguageError,
} from "./codeLspSession.svelte";

describe("planLspReconnect", () => {
  it("schedules bounded backoff retries", () => {
    expect(
      planLspReconnect({
        previousAttempt: 0,
        detail: "gone",
        reconnectDelay: (n) => (n === 1 ? 250 : n === 2 ? 750 : null),
        maxAttempts: 2,
      }),
    ).toEqual({
      action: "retry",
      attempt: 1,
      delayMs: 250,
      detail: "gone · retry 1/2",
    });
  });

  it("fails when attempts are exhausted", () => {
    expect(
      planLspReconnect({
        previousAttempt: 2,
        detail: "still down",
        reconnectDelay: () => null,
        maxAttempts: 2,
      }),
    ).toEqual({ action: "fail", detail: "still down" });
  });

  it("supports immediate reconnect with zero delay", () => {
    expect(
      planLspReconnect({
        previousAttempt: 0,
        detail: "Restarting language server",
        immediate: true,
        reconnectDelay: () => 999,
        maxAttempts: 3,
      }),
    ).toMatchObject({ action: "retry", delayMs: 0, attempt: 1 });
  });
});

describe("unusableLanguageError", () => {
  it("mentions the package when one is known", () => {
    expect(
      unusableLanguageError(
        {
          language: "rust",
          command: "rust-analyzer",
          binaryAvailable: false,
          usable: false,
          packageId: "rust-analyzer",
          rootMarkers: [],
          extensions: [],
          args: [],
        },
        "rust",
      ),
    ).toBe("rust-analyzer is not installed on this workshop");
  });

  it("falls back to PATH messaging", () => {
    expect(
      unusableLanguageError(
        {
          language: "go",
          command: "gopls",
          binaryAvailable: false,
          usable: false,
          packageId: null,
          rootMarkers: [],
          extensions: [],
          args: [],
        },
        "go",
      ),
    ).toBe("gopls was not found on this workshop PATH");
  });
});

function mockLease(overrides?: Partial<CodeWorkspaceLspLease>): CodeWorkspaceLspLease {
  const listeners = new Set<(status: CodeWorkspaceLspStatus) => void>();
  return {
    client: Promise.resolve({} as never),
    workspaceBridge: {
      register: () => () => {},
    } as never,
    subscribeStatus: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    restart: vi.fn(),
    release: vi.fn(),
    ...overrides,
  };
}

describe("CodeLspSession", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("fails permanently when the language matrix entry is unusable", async () => {
    const session = new CodeLspSession({
      getMatrix: async () => [
        {
          language: "rust",
          command: "rust-analyzer",
          binaryAvailable: false,
          usable: false,
          packageId: "rust-analyzer",
          rootMarkers: [],
          extensions: [],
          args: [],
        },
      ],
      acquire: async () => {
        throw new Error("should not acquire");
      },
      deferWork: (fn) => {
        fn();
        return () => {};
      },
    });

    session.connect({
      workId: "w1",
      workspaceRoot: "/repo",
      language: "rust",
      languageLabel: "rust",
      documentUri: "file:///repo/src/main.rs",
      bridge: {},
    });

    await Promise.resolve();
    await Promise.resolve();

    expect(session.status.phase).toBe("failed");
    expect(session.error).toContain("not installed");
    expect(session.client).toBeNull();
    session.dispose();
  });

  it("retries on acquire failure then fails after max attempts", async () => {
    vi.useFakeTimers();
    const timers: Array<{ id: number; fn: () => void; ms: number }> = [];
    let timerId = 0;
    const acquire = vi.fn(async () => {
      throw new Error("transport down");
    });

    const session = new CodeLspSession({
      getMatrix: async () => [],
      acquire,
      reconnectDelay: (n) => (n <= 2 ? 10 * n : null),
      maxReconnectAttempts: 2,
      deferWork: (fn) => {
        fn();
        return () => {};
      },
      setTimeout: (fn, ms) => {
        const id = ++timerId;
        timers.push({ id, fn, ms });
        return id as unknown as ReturnType<typeof setTimeout>;
      },
      clearTimeout: (id) => {
        const idx = timers.findIndex((t) => t.id === (id as unknown as number));
        if (idx >= 0) timers.splice(idx, 1);
      },
    });

    session.connect({
      workId: "w1",
      workspaceRoot: "/repo",
      language: "typescript",
      languageLabel: "typescript",
      documentUri: "file:///repo/a.ts",
      bridge: {},
    });

    await Promise.resolve();
    await Promise.resolve();
    expect(acquire).toHaveBeenCalledTimes(1);
    expect(session.status.phase).toBe("reconnecting");

    // Flush first reconnect timer
    const first = timers.shift();
    expect(first?.ms).toBe(10);
    first?.fn();
    await Promise.resolve();
    await Promise.resolve();
    expect(acquire).toHaveBeenCalledTimes(2);

    const second = timers.shift();
    expect(second?.ms).toBe(20);
    second?.fn();
    await Promise.resolve();
    await Promise.resolve();
    expect(acquire).toHaveBeenCalledTimes(3);
    expect(session.status.phase).toBe("failed");
    expect(session.error).toContain("transport down");

    session.dispose();
  });

  it("cancels an in-flight connect when scope changes mid-await", async () => {
    let resolveClient!: (value: unknown) => void;
    const clientPromise = new Promise((resolve) => {
      resolveClient = resolve;
    });
    const release = vi.fn();
    const acquire = vi.fn(async () =>
      mockLease({
        client: clientPromise as Promise<never>,
        release,
      }),
    );

    const session = new CodeLspSession({
      getMatrix: async () => [],
      acquire,
      deferWork: (fn) => {
        fn();
        return () => {};
      },
    });

    session.connect({
      workId: "w1",
      workspaceRoot: "/repo",
      language: "typescript",
      languageLabel: "typescript",
      documentUri: "file:///repo/a.ts",
      bridge: {},
    });
    await Promise.resolve();
    await Promise.resolve();

    session.connect({
      workId: "w1",
      workspaceRoot: "/repo",
      language: "typescript",
      languageLabel: "typescript",
      documentUri: "file:///repo/b.ts",
      bridge: {},
    });
    await Promise.resolve();

    resolveClient({});
    await Promise.resolve();
    await Promise.resolve();

    // First lease must be released after cancellation; second connect still pending client.
    expect(release).toHaveBeenCalled();
    session.dispose();
  });
});
