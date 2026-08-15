import {
  isBackgroundHandoffEvent,
  isTurnStreamTerminal,
  readSse,
  streamPathWithSince,
  TURN_STREAM_V2_MEDIA_TYPE,
} from "./stream.js";
import type {
  AgentModeId,
  AgentModeListResponse,
  AgentModeProposalListResponse,
  AgentModeProposalResponse,
  AgentModeScope,
  CapabilityListResponse,
  ClientRegistrationRequest,
  ClientRegistrationResponse,
  ClientRequestOptions,
  ClientToolRequest,
  ClientToolResultRequest,
  ClientToolResultResponse,
  ClientOptions,
  CreateSessionRequest,
  CreateSessionResponse,
  HealthResponse,
  ForgeUndertaking,
  InteractiveTurnRequest,
  InteractiveTurnResponse,
  InteractiveTurnStreamEvent,
  TurnStreamEnvelopeV2,
  SessionSummary,
  SessionSetDisplayNameResponse,
  SessionDeleteResponse,
  VaultBacklinksResponse,
  VaultNoteContentResponse,
  VaultSearchResponse,
  VaultWriteRequest,
  VaultWriteResponse,
  RuntimeDefaults,
  SessionHistoryResponse,
  SessionAgentModeResponse,
  SessionCodeBindingResponse,
  SessionCodeProjectResponse,
  StartSessionCodeProjectRequest,
  StreamOptions,
} from "./types.js";

export class MedousaHttpError extends Error {
  constructor(
    readonly status: number,
    readonly path: string,
    readonly body: string,
  ) {
    super(`Medousa request failed (${status}) ${path}`);
    this.name = "MedousaHttpError";
  }
}

export class MedousaClient {
  private readonly baseUrl: string;
  private readonly bearerToken?: string;
  private readonly fetchImpl: typeof globalThis.fetch;

  constructor(options: ClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.bearerToken = options.bearerToken;
    const fetchImpl = options.fetch ?? globalThis.fetch;
    if (!fetchImpl) throw new Error("A fetch implementation is required");
    // Browser fetch is an IDL method and must retain its Window/globalThis receiver.
    this.fetchImpl = fetchImpl.bind(globalThis);
  }

  async health(options?: ClientRequestOptions): Promise<HealthResponse> {
    return this.request<HealthResponse>("/health", { signal: options?.signal });
  }

  async registerClient(
    request: ClientRegistrationRequest,
    options?: ClientRequestOptions,
  ): Promise<ClientRegistrationResponse> {
    return this.request<ClientRegistrationResponse>("/v1/clients/register", {
      method: "POST",
      body: JSON.stringify(request),
      signal: options?.signal,
    });
  }

  async nextClientToolRequest(
    clientId: string,
    waitMs = 25_000,
    options?: ClientRequestOptions,
  ): Promise<ClientToolRequest | null> {
    const params = new URLSearchParams({
      wait_ms: String(Math.max(0, Math.min(waitMs, 30_000))),
    });
    return this.request<ClientToolRequest | null>(
      `/v1/clients/${encodeURIComponent(clientId)}/tools/next?${params.toString()}`,
      { signal: options?.signal },
    );
  }

  async completeClientToolRequest(
    clientId: string,
    requestId: string,
    request: ClientToolResultRequest,
    options?: ClientRequestOptions,
  ): Promise<ClientToolResultResponse> {
    return this.request<ClientToolResultResponse>(
      `/v1/clients/${encodeURIComponent(clientId)}/tools/${encodeURIComponent(requestId)}/result`,
      {
        method: "POST",
        body: JSON.stringify(request),
        signal: options?.signal,
      },
    );
  }

  async capabilities(options?: ClientRequestOptions): Promise<CapabilityListResponse> {
    return this.request<CapabilityListResponse>("/v1/capabilities", {
      signal: options?.signal,
    });
  }

  async sessions(limit = 20, options?: ClientRequestOptions): Promise<SessionSummary[]> {
    const response = await this.request<unknown>(`/v1/sessions?limit=${limit}`, {
      signal: options?.signal,
    });
    if (Array.isArray(response)) return response as SessionSummary[];
    if (response && typeof response === "object" && "sessions" in response) {
      const sessions = (response as { sessions?: unknown }).sessions;
      return Array.isArray(sessions) ? (sessions as SessionSummary[]) : [];
    }
    return [];
  }

  async createSession(
    request: CreateSessionRequest = {},
    options?: ClientRequestOptions,
  ): Promise<CreateSessionResponse> {
    return this.request<CreateSessionResponse>("/v1/sessions", {
      method: "POST",
      body: JSON.stringify(request),
      signal: options?.signal,
    });
  }

  async sessionHistory(
    sessionId: string,
    options?: ClientRequestOptions,
  ): Promise<SessionHistoryResponse> {
    return this.request<SessionHistoryResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/history`,
      { signal: options?.signal },
    );
  }

  async agentModes(options?: ClientRequestOptions): Promise<AgentModeListResponse> {
    return this.request<AgentModeListResponse>("/v1/agent-modes", {
      signal: options?.signal,
    });
  }

  async sessionAgentMode(
    sessionId: string,
    options?: ClientRequestOptions,
  ): Promise<SessionAgentModeResponse> {
    return this.request<SessionAgentModeResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/agent-mode`,
      { signal: options?.signal },
    );
  }

  async setSessionAgentMode(
    sessionId: string,
    mode: AgentModeId,
    scope: AgentModeScope = "session",
    taskId?: string,
    options?: ClientRequestOptions,
  ): Promise<SessionAgentModeResponse> {
    return this.request<SessionAgentModeResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/agent-mode`,
      {
        method: "PUT",
        body: JSON.stringify({ mode, scope, task_id: taskId }),
        signal: options?.signal,
      },
    );
  }

  async agentModeProposals(
    sessionId: string,
    options?: ClientRequestOptions,
  ): Promise<AgentModeProposalListResponse> {
    return this.request<AgentModeProposalListResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/agent-mode/proposals`,
      { signal: options?.signal },
    );
  }

  async decideAgentModeProposal(
    sessionId: string,
    proposalId: string,
    accept: boolean,
    options?: ClientRequestOptions,
  ): Promise<AgentModeProposalResponse> {
    return this.request<AgentModeProposalResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/agent-mode/proposals/${encodeURIComponent(proposalId)}`,
      {
        method: "PUT",
        body: JSON.stringify({ accept }),
        signal: options?.signal,
      },
    );
  }

  async sessionCodeBinding(
    sessionId: string,
    options?: ClientRequestOptions,
  ): Promise<SessionCodeBindingResponse> {
    return this.request<SessionCodeBindingResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/code-binding`,
      { signal: options?.signal },
    );
  }

  async setSessionCodeBinding(
    sessionId: string,
    workId: string,
    options?: ClientRequestOptions,
  ): Promise<SessionCodeBindingResponse> {
    return this.request<SessionCodeBindingResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/code-binding`,
      {
        method: "PUT",
        body: JSON.stringify({ work_id: workId }),
        signal: options?.signal,
      },
    );
  }

  async clearSessionCodeBinding(
    sessionId: string,
    options?: ClientRequestOptions,
  ): Promise<SessionCodeBindingResponse> {
    return this.request<SessionCodeBindingResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/code-binding`,
      { method: "DELETE", signal: options?.signal },
    );
  }

  async startSessionCodeProject(
    sessionId: string,
    request: StartSessionCodeProjectRequest,
    options?: ClientRequestOptions,
  ): Promise<SessionCodeProjectResponse> {
    return this.request<SessionCodeProjectResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/code-project`,
      {
        method: "POST",
        body: JSON.stringify(request),
        signal: options?.signal,
      },
    );
  }

  async forgeUndertakings(options?: ClientRequestOptions): Promise<ForgeUndertaking[]> {
    return this.request<ForgeUndertaking[]>("/v1/forge/items", {
      signal: options?.signal,
    });
  }

  async forgeUndertaking(
    workId: string,
    options?: ClientRequestOptions,
  ): Promise<ForgeUndertaking> {
    return this.request<ForgeUndertaking>(
      `/v1/forge/items/${encodeURIComponent(workId)}`,
      { signal: options?.signal },
    );
  }

  async renameSession(
    sessionId: string,
    displayName: string,
    options?: ClientRequestOptions,
  ): Promise<SessionSetDisplayNameResponse> {
    return this.request<SessionSetDisplayNameResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}/name`,
      {
        method: "PUT",
        body: JSON.stringify({ display_name: displayName }),
        signal: options?.signal,
      },
    );
  }

  async deleteSession(
    sessionId: string,
    purgeMemory = true,
    options?: ClientRequestOptions,
  ): Promise<SessionDeleteResponse> {
    return this.request<SessionDeleteResponse>(
      `/v1/sessions/${encodeURIComponent(sessionId)}?purge_memory=${purgeMemory}`,
      { method: "DELETE", signal: options?.signal },
    );
  }

  async createVaultNote(
    request: VaultWriteRequest,
    options?: ClientRequestOptions,
  ): Promise<VaultWriteResponse> {
    return this.request<VaultWriteResponse>("/v1/vault/notes", {
      method: "POST",
      body: JSON.stringify(request),
      signal: options?.signal,
    });
  }

  async getVaultNote(
    path: string,
    options?: ClientRequestOptions,
  ): Promise<VaultNoteContentResponse> {
    return this.request<VaultNoteContentResponse>(`/v1/vault/notes/${encodeVaultPath(path)}`, {
      signal: options?.signal,
    });
  }

  async updateVaultNote(
    path: string,
    content: string,
    ifMatch?: string,
    options?: ClientRequestOptions,
  ): Promise<VaultWriteResponse> {
    return this.request<VaultWriteResponse>(`/v1/vault/notes/${encodeVaultPath(path)}`, {
      method: "PUT",
      body: content,
      headers: {
        "Content-Type": "text/markdown; charset=utf-8",
        ...(ifMatch ? { "If-Match": ifMatch } : {}),
      },
      signal: options?.signal,
    });
  }

  async searchVault(
    query: string,
    limit = 20,
    options?: ClientRequestOptions,
  ): Promise<VaultSearchResponse> {
    const params = new URLSearchParams({ q: query, limit: String(limit) });
    return this.request<VaultSearchResponse>(`/v1/vault/search?${params.toString()}`, {
      signal: options?.signal,
    });
  }

  async vaultBacklinks(
    path: string,
    options?: ClientRequestOptions,
  ): Promise<VaultBacklinksResponse> {
    const params = new URLSearchParams({ path });
    return this.request<VaultBacklinksResponse>(`/v1/vault/backlinks?${params.toString()}`, {
      signal: options?.signal,
    });
  }

  async runtimeDefaults(options?: ClientRequestOptions): Promise<RuntimeDefaults> {
    return this.request<RuntimeDefaults>("/v1/runtime/defaults", {
      signal: options?.signal,
    });
  }

  async startTurn(
    request: InteractiveTurnRequest,
    options?: ClientRequestOptions,
  ): Promise<InteractiveTurnResponse> {
    return this.request<InteractiveTurnResponse>("/v1/interactive/turn", {
      method: "POST",
      body: JSON.stringify(request),
      signal: options?.signal,
    });
  }

  async cancelTurn(sessionId: string, options?: ClientRequestOptions): Promise<void> {
    await this.request<unknown>(`/v1/sessions/${encodeURIComponent(sessionId)}/active-turn`, {
      method: "POST",
      body: JSON.stringify({ cancel: true }),
      signal: options?.signal,
    });
  }

  async approveBudget(requestId: string, extraRounds?: number, resolvedBy = "vscode"): Promise<void> {
    await this.request<unknown>(
      `/v1/turns/budget-requests/${encodeURIComponent(requestId)}/approve`,
      { method: "POST", body: JSON.stringify({ extra_rounds: extraRounds, resolved_by: resolvedBy }) },
    );
  }

  async denyBudget(requestId: string, resolvedBy = "vscode"): Promise<void> {
    await this.request<unknown>(
      `/v1/turns/budget-requests/${encodeURIComponent(requestId)}/deny`,
      { method: "POST", body: JSON.stringify({ resolved_by: resolvedBy }) },
    );
  }

  async resolvePermission(requestId: string, approve: boolean, resolvedBy = "vscode"): Promise<void> {
    const action = approve ? "approve" : "deny";
    await this.request<unknown>(
      `/v1/agents/permission-requests/${encodeURIComponent(requestId)}/${action}`,
      { method: "POST", body: JSON.stringify({ resolved_by: resolvedBy }) },
    );
  }

  async *streamTurn(
    response: InteractiveTurnResponse,
    options: StreamOptions = {},
  ): AsyncGenerator<InteractiveTurnStreamEvent> {
    let path = response.stream_url;
    let lastSeq = 0;
    let attempt = 0;
    const maxAttempts = options.maxReconnectAttempts ?? 10;
    const delay = options.reconnectDelayMs ?? ((current: number) => Math.min(500 * 2 ** current, 30_000));

    while (true) {
      if (options.signal?.aborted) return;
      const streamPath = streamPathWithSince(path, lastSeq);
      const streamResponse = await this.fetchImpl(this.resolve(streamPath), {
        headers: this.headers(),
        signal: options.signal,
      });

      if (!streamResponse.ok) {
        const body = await streamResponse.text();
        if (attempt >= maxAttempts) throw new MedousaHttpError(streamResponse.status, streamPath, body);
        await this.sleep(delay(attempt++), options.signal);
        continue;
      }

      try {
        for await (const event of readSse(streamResponse)) {
          if (event.seq && event.seq <= lastSeq) continue;
          if (event.seq) lastSeq = event.seq;
          attempt = 0;
          yield event;
          if (event.terminal || (options.stopOnHandoff && isBackgroundHandoffEvent(event))) return;
        }
      } catch (error) {
        if (options.signal?.aborted) return;
        if (attempt >= maxAttempts) throw error;
      }

      if (attempt >= maxAttempts) throw new Error("Medousa stream reconnect limit reached");
      await this.sleep(delay(attempt++), options.signal);
      path = response.stream_url;
    }
  }

  async *streamTurnV2(
    response: InteractiveTurnResponse,
    options: StreamOptions = {},
  ): AsyncGenerator<TurnStreamEnvelopeV2> {
    let path = response.stream_url;
    let lastSeq = 0;
    let attempt = 0;
    const maxAttempts = options.maxReconnectAttempts ?? 10;
    const delay =
      options.reconnectDelayMs ?? ((current: number) => Math.min(500 * 2 ** current, 30_000));

    while (true) {
      if (options.signal?.aborted) return;
      const streamPath = streamPathWithSince(path, lastSeq);
      const streamResponse = await this.fetchImpl(this.resolve(streamPath), {
        headers: { ...this.headers(), Accept: TURN_STREAM_V2_MEDIA_TYPE },
        signal: options.signal,
      });

      if (!streamResponse.ok) {
        const body = await streamResponse.text();
        if (attempt >= maxAttempts) {
          throw new MedousaHttpError(streamResponse.status, streamPath, body);
        }
        await this.sleep(delay(attempt++), options.signal);
        continue;
      }

      try {
        for await (const envelope of readSse<TurnStreamEnvelopeV2>(streamResponse)) {
          if (envelope.schema_version !== 2) {
            throw new Error(`Unsupported Medousa turn stream version ${envelope.schema_version}`);
          }
          if (envelope.seq <= lastSeq) continue;
          lastSeq = envelope.seq;
          attempt = 0;
          yield envelope;
          if (
            isTurnStreamTerminal(envelope) ||
            (options.stopOnHandoff && isBackgroundHandoffEvent(envelope))
          ) {
            return;
          }
        }
      } catch (error) {
        if (options.signal?.aborted) return;
        if (attempt >= maxAttempts) throw error;
      }

      if (attempt >= maxAttempts) throw new Error("Medousa stream reconnect limit reached");
      await this.sleep(delay(attempt++), options.signal);
      path = response.stream_url;
    }
  }

  private async request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const response = await this.fetchImpl(this.resolve(path), {
      ...init,
      headers: {
        ...this.headers(),
        ...(init.body ? { "Content-Type": "application/json" } : {}),
        ...(init.headers ?? {}),
      },
    });
    if (!response.ok) throw new MedousaHttpError(response.status, path, await response.text());
    if (response.status === 204) return undefined as T;
    return (await response.json()) as T;
  }

  private headers(): Record<string, string> {
    return {
      Accept: "application/json",
      ...(this.bearerToken ? { Authorization: `Bearer ${this.bearerToken}` } : {}),
    };
  }

  private resolve(path: string): string {
    return new URL(path, `${this.baseUrl}/`).toString();
  }

  private async sleep(ms: number, signal?: AbortSignal): Promise<void> {
    if (signal?.aborted) return;
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(resolve, ms);
      signal?.addEventListener("abort", () => {
        clearTimeout(timer);
        reject(signal.reason ?? new Error("Aborted"));
      }, { once: true });
    });
  }
}

function encodeVaultPath(path: string): string {
  return path
    .split("/")
    .filter(Boolean)
    .map((segment) => encodeURIComponent(segment))
    .join("/");
}
