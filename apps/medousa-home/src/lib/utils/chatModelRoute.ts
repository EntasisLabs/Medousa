import { isExternalRuntime, externalAgent } from "$lib/utils/externalAgentCatalog";
import type { ExternalAgentRuntime } from "$lib/utils/externalAgentCatalog";
import type { AgentSessionConfigOption } from "$lib/daemon";
import type { ChatAgentRuntime } from "$lib/utils/sessionAgentRuntime";

export type ChatCredentialRoute =
  | "api-key"
  | "local"
  | "chatgpt-account"
  | "cursor-account"
  | "hermes-account"
  | "external-agent-account";

export interface ChatModelRouteRef {
  runtime: ChatAgentRuntime;
  provider: string;
  credential: ChatCredentialRoute;
  model: string;
}

export function chatModelRouteKey(route: ChatModelRouteRef): string {
  return [route.runtime, route.provider, route.credential, route.model]
    .map((part) => part.trim().toLowerCase())
    .join("/");
}

export function credentialRouteFor(
  runtime: ChatAgentRuntime,
  provider: string,
): ChatCredentialRoute {
  if (runtime === "codex") return "chatgpt-account";
  if (runtime === "cursor") return "cursor-account";
  if (runtime === "hermes") return "hermes-account";
  if (isExternalRuntime(runtime)) return "external-agent-account";
  const normalized = provider.trim().toLowerCase();
  if (normalized === "openai-codex") return "chatgpt-account";
  return normalized === "ollama" || normalized === "medousa-local"
    ? "local"
    : "api-key";
}

export function agentModelConfigOption(
  options: AgentSessionConfigOption[],
): AgentSessionConfigOption | null {
  return (
    options.find(
      (option) =>
        option.type === "select" &&
        (option.id === "model" || option.category === "model"),
    ) ?? null
  );
}

export function agentModelDisplayLabel(
  runtime: ExternalAgentRuntime,
  options: AgentSessionConfigOption[],
): string {
  const option = agentModelConfigOption(options);
  if (option) {
    const selected = option.options?.find((choice) => choice.value === option.currentValue);
    if (selected?.name.trim()) return selected.name.trim();
    if (typeof option.currentValue === "string" && option.currentValue.trim()) {
      return option.currentValue.trim();
    }
  }
  if (runtime === "codex") return "Choose ChatGPT model";
  if (runtime === "hermes") return "Choose Hermes model";
  return `Choose ${externalAgent(runtime).label} model`;
}

export function modelSourceLabel(runtime: ChatAgentRuntime): string {
  if (isExternalRuntime(runtime)) return runtime === "codex" ? "ChatGPT" : externalAgent(runtime).label;
  switch (runtime) {
    case "instinct":
      return "Instinct Agent";
    case "dots":
      return "Dots";
    case "muse":
      return "Muse";
    case "grok_bot":
      return "Grok Bot";
    default:
      return "Medousa";
  }
}

export function modelSourceDetail(
  runtime: ChatAgentRuntime,
  nativeProviderLabel: string,
  nativeProviderId: string,
): string {
  if (runtime === "codex") return "OpenAI account · Codex runtime";
  if (runtime === "cursor") return "Cursor account · Cursor runtime";
  if (runtime === "hermes") return "Hermes providers · Hermes runtime";
  if (isExternalRuntime(runtime)) return `${externalAgent(runtime).label} · Workshop agent`;
  const provider = nativeProviderId.trim().toLowerCase();
  const connection = credentialRouteFor(runtime, provider) === "local" ? "Local" : "API key";
  return `${nativeProviderLabel} · ${connection}`;
}
