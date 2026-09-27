import { daemonUnary } from "./contractClient";
import type {
  CreateExternalConversationRequest,
  CreateExternalConversationResponse,
  ExternalConversationListResponse,
  ExternalConversationView,
  RotateExternalCallbackResponse,
  DeleteExternalConversationResponse,
} from "$lib/types/generated/daemon_api";

export type { ExternalProvider } from "$lib/types/generated/daemon_api";
export type ExternalConversation = ExternalConversationView;

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
