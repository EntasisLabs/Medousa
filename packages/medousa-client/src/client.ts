import { readSse, streamPathWithSince } from "./stream.js";
import type {
  CapabilityListResponse,
  ClientOptions,
  ClientRequestOptions,
  HealthResponse,
  InteractiveTurnRequest,
  InteractiveTurnResponse,
  InteractiveTurnStreamEvent,
  SessionSummary,
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
    this.fetchImpl = options.fetch ?? globalThis.fetch;
    if (!this.fetchImpl) throw new Error("A fetch implementation is required");
  }

  async health(options?: ClientRequestOptions): Promise<HealthResponse> {
    return this.request<HealthResponse>("/health", { signal: options?.signal });
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
          if (event.terminal) return;
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
