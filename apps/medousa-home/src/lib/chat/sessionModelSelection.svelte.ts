import type { ChatMessage } from "$lib/types/chat";
import type { StageRoutingMatrix } from "$lib/types/runtime";
import { alignStageRoutingWithHost, defaultStageRouting } from "$lib/utils/stageRouting";

export interface ChatModelSelection {
  provider: string;
  model: string;
  stageRouting: StageRoutingMatrix;
}

export interface ChatModelContext {
  sessionId: string;
  workshopScopeId: string;
  messages: readonly ChatMessage[];
}

export type SessionScope = Pick<ChatModelContext, "sessionId" | "workshopScopeId">;

function storageKey(scope: SessionScope): string {
  return `medousa-home-chat-model:${JSON.stringify([scope.workshopScopeId, scope.sessionId])}`;
}

function nonempty(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function parseSelection(raw: string | null): ChatModelSelection | null {
  if (!raw) return null;
  try {
    const value = JSON.parse(raw);
    if (value?.version !== 1 || !nonempty(value.provider) || !nonempty(value.model)) return null;
    const routes = value.stageRouting;
    for (const role of Object.keys(defaultStageRouting(value.provider, value.model))) {
      const route = routes?.[role];
      if (!route || route.role !== role || !nonempty(route.provider) || !nonempty(route.model)
        || !nonempty(route.policy_profile) || !Array.isArray(route.fallback_chain)
        || !route.fallback_chain.every(nonempty)) return null;
    }
    return { provider: value.provider, model: value.model, stageRouting: routes };
  } catch {
    return null;
  }
}

/** Local composer preferences: workshop + session scoped, independent of engine defaults. */
export class SessionModelSelections {
  private selections = $state<Record<string, ChatModelSelection | null>>({});

  get(scope: SessionScope): ChatModelSelection | null {
    const key = storageKey(scope);
    if (Object.hasOwn(this.selections, key)) return this.selections[key];
    try {
      return parseSelection(localStorage.getItem(key));
    } catch {
      return null;
    }
  }

  resolve(context: ChatModelContext, defaults: ChatModelSelection | null): ChatModelSelection | null {
    const saved = this.get(context);
    if (saved) return saved;
    // Worker/ask receipts must not change the principal conversation's model.
    const receipt = context.messages.findLast((message) =>
      message.role === "assistant" && (!message.lane || message.lane === "chat")
      && nonempty(message.responseProvider) && nonempty(message.responseModel),
    );
    if (receipt) {
      const provider = receipt.responseProvider!.trim();
      const model = receipt.responseModel!.trim();
      return {
        provider,
        model,
        stageRouting: alignStageRoutingWithHost(defaults?.stageRouting, provider, model),
      };
    }
    return defaults ? {
      provider: defaults.provider,
      model: defaults.model,
      stageRouting: defaults.stageRouting,
    } : null;
  }

  set(scope: SessionScope, selection: ChatModelSelection): void {
    if (!scope.sessionId.trim() || !scope.workshopScopeId.trim()) return;
    const key = storageKey(scope);
    // Detach from reactive shared defaults so subsequent settings changes cannot move this chat.
    const snapshot = JSON.parse(JSON.stringify(selection)) as ChatModelSelection;
    this.selections[key] = snapshot;
    try {
      localStorage.setItem(key, JSON.stringify({ version: 1, ...snapshot }));
    } catch {
      // Keep the selection for this app lifetime even when storage is unavailable.
    }
  }

  clear(scope: SessionScope): void {
    const key = storageKey(scope);
    this.selections[key] = null;
    try {
      localStorage.removeItem(key);
    } catch {
      // The in-memory preference has still been removed.
    }
  }
}

export const sessionModelSelections = new SessionModelSelections();
