/** Failures stay actionable even when the user is viewing the conversation. */
export function shouldNotifyTurnCompletion(failed: boolean, siriOwnsResult: boolean, viewingConversation = false): boolean {
  return failed || !(siriOwnsResult || viewingConversation);
}
