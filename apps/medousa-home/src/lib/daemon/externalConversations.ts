import { daemonUnary } from "./contractClient";
import type {
  CreateExternalAgentTokenRequest,
  CreateExternalAgentTokenResponse,
  CreateExternalConversationRequest,
  CreateExternalConversationResponse,
  ExternalConversationListResponse,
  ExternalConversationView,
  RotateExternalCallbackResponse,
  DeleteExternalConversationResponse,
  ExternalMuseDiscoveryStatus,
  ExternalWhatsAppPairingStatus,
} from "$lib/types/generated/daemon_api";

export type { ExternalProvider } from "$lib/types/generated/daemon_api";
export type ExternalConversation = ExternalConversationView;

export function startMuseDiscovery(): Promise<ExternalMuseDiscoveryStatus> {
  return daemonUnary("external_conversations.muse.discovery.post");
}

export function getMuseDiscovery(): Promise<ExternalMuseDiscoveryStatus> {
  return daemonUnary("external_conversations.muse.discovery.get");
}

export function getWhatsAppPairingStatus(): Promise<ExternalWhatsAppPairingStatus> {
  return daemonUnary("external_conversations.whatsapp.pairing.get");
}

export async function listExternalConversations(): Promise<ExternalConversation[]> {
  const result = await daemonUnary<ExternalConversationListResponse>(
    "external_conversations.get",
  );
  return result.conversations;
}

export function getExternalConversation(id: string): Promise<ExternalConversation> {
  return daemonUnary("external_conversations.by_id.get", { id });
}

export function createExternalConversation(request: CreateExternalConversationRequest): Promise<CreateExternalConversationResponse> {
  return daemonUnary("external_conversations.post", {}, request);
}

export function sendExternalConversationMessage(
  id: string,
  text: string,
  requestId: string,
): Promise<ExternalConversation> {
  return daemonUnary("external_conversations.by_id.messages.post", { id }, {
    request_id: requestId,
    text,
  });
}

export function rotateExternalCallbackKey(id: string): Promise<RotateExternalCallbackResponse> {
  return daemonUnary("external_conversations.by_id.callback_key.rotate.post", { id });
}

export function deleteExternalConversation(id: string): Promise<DeleteExternalConversationResponse> {
  return daemonUnary("external_conversations.by_id.delete", { id });
}

export function createExternalAgentToken(id: string, request: CreateExternalAgentTokenRequest): Promise<CreateExternalAgentTokenResponse> {
  return daemonUnary("external_conversations.by_id.api_token.post", { id }, request);
}

export function revokeExternalAgentToken(id: string): Promise<ExternalConversation> {
  return daemonUnary("external_conversations.by_id.api_token.delete", { id });
}
