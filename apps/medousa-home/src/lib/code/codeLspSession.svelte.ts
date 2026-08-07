/**
 * Explicit LSP connection state machine for the Code editor.
 * Owns backoff, attempt counting, and scope changes — callers no longer bump
 * an `lspRetry` integer to re-trigger a reactive effect.
 */

import type { LSPClient } from "@codemirror/lsp-client";
import {
  CODE_LSP_MAX_RECONNECT_ATTEMPTS,
  acquireCodeWorkspaceLspClient,
  codeLspReconnectDelay,
  findCodeLanguageMatrixEntry,
  getCodeLanguageMatrix,
  isPermanentLanguageServiceError,
  type CodeLanguageMatrixEntry,
  type CodeWorkspaceLspLease,
  type CodeWorkspaceLspStatus,
} from "$lib/code/codingEngineClient";
import type { MedousaCodeWorkspaceHandler } from "$lib/code/medousaCodeWorkspace";
import { deferCodeWorkspaceWork } from "$lib/utils/codeWorkspaceTrace";

export type CodeLspSessionBridge = MedousaCodeWorkspaceHandler;

export type CodeLspConnectRequest = {
  workId: string;
  workspaceRoot: string;
  language: string;
  /** Human-readable language id for status copy (e.g. active tab language). */
  languageLabel: string;
  documentUri: string;
  bridge: CodeLspSessionBridge;
};

export type CodeLspSessionDeps = {
  acquire: (options: {
    workId: string;
    workspaceRoot: string;
    language: string;
    documentUri: string;
  }) => Promise<CodeWorkspaceLspLease>;
  getMatrix: () => Promise<CodeLanguageMatrixEntry[]>;
  findMatrixEntry: typeof findCodeLanguageMatrixEntry;
  reconnectDelay: (attempt: number) => number | null;
  maxReconnectAttempts: number;
  deferWork: (fn: () => void) => () => void;
  setTimeout: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  clearTimeout: (id: ReturnType<typeof setTimeout>) => void;
};

const STOPPED: CodeWorkspaceLspStatus = {
  phase: "stopped",
  detail: "Language service is not running",
  progress: null,
};

export function unusableLanguageError(
  entry: CodeLanguageMatrixEntry,
  language: string,
): string {
  const missing = entry.command ?? language;
  return entry.packageId
    ? `${missing} is not installed on this workshop`
    : `${missing} was not found on this workshop PATH`;
}

export { isPermanentLanguageServiceError };

export type CodeLspReconnectPlan =
  | { action: "retry"; attempt: number; delayMs: number; detail: string }
  | { action: "fail"; detail: string };

/** Pure decision for the next reconnect step (no timers / I/O). */
export function planLspReconnect(options: {
  previousAttempt: number;
  detail: string;
  immediate?: boolean;
  permanent?: boolean;
  reconnectDelay?: (attempt: number) => number | null;
  maxAttempts?: number;
}): CodeLspReconnectPlan {
  if (options.permanent || isPermanentLanguageServiceError(options.detail)) {
    return {
      action: "fail",
      detail: options.detail || "Language server could not be restarted",
    };
  }
  const attempt = options.previousAttempt + 1;
  const delayFn = options.reconnectDelay ?? codeLspReconnectDelay;
  const max = options.maxAttempts ?? CODE_LSP_MAX_RECONNECT_ATTEMPTS;
  const delay = options.immediate ? 0 : delayFn(attempt);
  if (delay == null || attempt > max) {
    return {
      action: "fail",
      detail: options.detail || "Language server could not be restarted",
    };
  }
  return {
    action: "retry",
    attempt,
    delayMs: delay,
    detail: `${options.detail} · retry ${attempt}/${max}`,
  };
}

const defaultDeps: CodeLspSessionDeps = {
  acquire: acquireCodeWorkspaceLspClient,
  getMatrix: getCodeLanguageMatrix,
  findMatrixEntry: findCodeLanguageMatrixEntry,
  reconnectDelay: codeLspReconnectDelay,
  maxReconnectAttempts: CODE_LSP_MAX_RECONNECT_ATTEMPTS,
  deferWork: deferCodeWorkspaceWork,
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (id) => clearTimeout(id),
};

export class CodeLspSession {
  client = $state<LSPClient | null>(null);
  status = $state<CodeWorkspaceLspStatus>({ ...STOPPED });
  error = $state<string | null>(null);
  connecting = $state(false);
  languageMatrix = $state<CodeLanguageMatrixEntry[]>([]);
  languageMatrixError = $state<string | null>(null);

  #deps: CodeLspSessionDeps;
  #scope = "";
  #attempt = 0;
  #generation = 0;
  #reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  #cancelDeferred: (() => void) | null = null;
  #release: (() => void) | null = null;
  #unregisterBridge: (() => void) | null = null;
  #unsubscribeStatus: (() => void) | null = null;
  #leaseRestart: (() => void) | null = null;
  #pending: CodeLspConnectRequest | null = null;

  constructor(deps?: Partial<CodeLspSessionDeps>) {
    this.#deps = { ...defaultDeps, ...deps };
  }

  /** Active connect scope (`workId:language:uri`), empty when stopped. */
  get scope(): string {
    return this.#scope;
  }

  get reconnectAttempt(): number {
    return this.#attempt;
  }

  connect(request: CodeLspConnectRequest): void {
    const scope = `${request.workId}:${request.language}:${request.documentUri}`;
    if (scope !== this.#scope) {
      this.#scope = scope;
      this.#attempt = 0;
      this.languageMatrix = [];
      this.languageMatrixError = null;
    } else if (
      this.#pending &&
      (this.status.phase === "connecting" ||
        this.status.phase === "reconnecting" ||
        this.status.phase === "ready")
    ) {
      // Same scope already live — refresh bridge handlers without tearing down.
      this.#pending = request;
      return;
    }
    this.#pending = request;
    this.#begin(request);
  }

  /** Idle / unsupported language — clear client and publish stopped. */
  stop(detail = STOPPED.detail): void {
    this.#pending = null;
    this.#scope = "";
    this.#attempt = 0;
    this.#teardownConnect();
    this.client = null;
    this.error = null;
    this.connecting = false;
    this.status = { phase: "stopped", detail, progress: null };
    this.#leaseRestart = null;
  }

  /** User-triggered restart (or after package repair). */
  restart(): void {
    this.error = null;
    this.#attempt = 0;
    if (this.#leaseRestart) {
      this.#leaseRestart();
      return;
    }
    if (this.#pending) this.#begin(this.#pending);
  }

  dispose(): void {
    this.stop();
  }

  #begin(request: CodeLspConnectRequest): void {
    this.#teardownConnect();
    const generation = ++this.#generation;
    this.client = null;
    this.error = null;
    this.connecting = true;
    this.status = {
      phase: this.#attempt > 0 ? "reconnecting" : "connecting",
      detail:
        this.#attempt > 0
          ? `Restarting ${request.languageLabel} language server`
          : `Starting ${request.languageLabel} language server`,
      progress: null,
    };

    this.#cancelDeferred = this.#deps.deferWork(() => {
      void this.#runConnect(request, generation);
    });
  }

  async #runConnect(request: CodeLspConnectRequest, generation: number): Promise<void> {
    const alive = () => generation === this.#generation;
    try {
      try {
        const matrix = await this.#deps.getMatrix();
        if (!alive()) return;
        this.languageMatrix = matrix;
        this.languageMatrixError = null;
        const entry = this.#deps.findMatrixEntry(matrix, request.language);
        if (entry && !entry.usable) {
          const detail = unusableLanguageError(entry, request.language);
          this.connecting = false;
          this.error = detail;
          this.status = { phase: "failed", detail, progress: null };
          return;
        }
      } catch (err) {
        // Older coding engines omit the matrix; keep attempting the LSP.
        if (alive()) {
          this.languageMatrixError =
            err instanceof Error ? err.message : String(err);
        }
      }

      const lease = await this.#deps.acquire({
        workId: request.workId,
        workspaceRoot: request.workspaceRoot,
        language: request.language,
        documentUri: request.documentUri,
      });
      if (!alive()) {
        lease.release();
        return;
      }

      this.#release = lease.release;
      this.#leaseRestart = lease.restart;
      this.#unsubscribeStatus = lease.subscribeStatus((status) => {
        if (!alive()) return;
        this.status = status;
        if (status.phase === "ready") {
          this.#attempt = 0;
          this.connecting = false;
          this.error = null;
          return;
        }
        if (status.phase === "connecting") {
          this.connecting = true;
          return;
        }
        if (status.phase === "reconnecting" || status.phase === "failed") {
          this.#scheduleReconnect(
            status.detail,
            status.detail === "Restarting language server",
          );
        }
      });
      this.#unregisterBridge = lease.workspaceBridge.register(request.bridge);

      const client = await lease.client;
      if (!alive()) return;
      this.client = client;
    } catch (err) {
      if (!alive()) return;
      this.#cleanupLeaseHooks();
      this.#scheduleReconnect(err instanceof Error ? err.message : String(err));
    }
  }

  #scheduleReconnect(detail: string, immediate = false): void {
    if (this.#reconnectTimer || !this.#pending) return;
    const plan = planLspReconnect({
      previousAttempt: this.#attempt,
      detail,
      immediate,
      reconnectDelay: this.#deps.reconnectDelay,
      maxAttempts: this.#deps.maxReconnectAttempts,
    });
    if (plan.action === "fail") {
      this.connecting = false;
      this.error = plan.detail;
      this.status = { phase: "failed", detail: plan.detail, progress: null };
      this.client = null;
      return;
    }
    this.#attempt = plan.attempt;
    this.client = null;
    this.error = null;
    this.connecting = true;
    this.status = {
      phase: "reconnecting",
      detail: plan.detail,
      progress: null,
    };
    const pending = this.#pending;
    this.#reconnectTimer = this.#deps.setTimeout(() => {
      this.#reconnectTimer = null;
      if (this.#pending === pending) this.#begin(pending);
    }, plan.delayMs);
  }

  #cleanupLeaseHooks(): void {
    this.#unsubscribeStatus?.();
    this.#unsubscribeStatus = null;
    this.#unregisterBridge?.();
    this.#unregisterBridge = null;
    this.#release?.();
    this.#release = null;
    this.#leaseRestart = null;
  }

  #teardownConnect(): void {
    this.#generation += 1;
    if (this.#reconnectTimer) {
      this.#deps.clearTimeout(this.#reconnectTimer);
      this.#reconnectTimer = null;
    }
    this.#cancelDeferred?.();
    this.#cancelDeferred = null;
    this.#cleanupLeaseHooks();
  }
}
