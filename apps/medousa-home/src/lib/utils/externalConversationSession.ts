import type { ExternalProvider } from "$lib/types/generated/daemon_api";

const PREFIX = "external-conversation:";
const PROVIDERS = new Set<ExternalProvider>(["muse", "grok_bot", "instinct", "dots"]);

/** A shell chat address, scoped by the active workshop; the bridge owns its transcript. */
export function externalConversationSessionId(provider: ExternalProvider, id: string): string {
  return `${PREFIX}${provider}:${encodeURIComponent(id.trim())}`;
}

export function externalConversationBinding(sessionId: string): { provider: ExternalProvider; id: string } | null {
  const value = sessionId.trim();
  if (!value.startsWith(PREFIX)) return null;
  const [provider, ...parts] = value.slice(PREFIX.length).split(":");
  if (!PROVIDERS.has(provider as ExternalProvider)) return null;
  try {
    const id = decodeURIComponent(parts.join(":"));
    return id.trim() ? { provider: provider as ExternalProvider, id } : null;
  } catch {
    return null;
  }
}
