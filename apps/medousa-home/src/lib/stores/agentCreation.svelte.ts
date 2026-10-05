import type { ExternalProvider } from "$lib/daemon/externalConversations";

class AgentCreationStore {
  kind = $state<"bot" | "connection" | null>(null);
  provider = $state<ExternalProvider | null>(null);
  createBot() { this.provider = null; this.kind = "bot"; }
  connectAgent(provider: ExternalProvider | null = null) { this.provider = provider; this.kind = "connection"; }
  close() { this.kind = null; this.provider = null; }
}
export const agentCreation = new AgentCreationStore();
