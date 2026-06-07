export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  streaming?: boolean;
}

export interface InteractiveTurnStreamEvent {
  turn_id: string;
  event_type: string;
  phase: string;
  message: string;
  content_delta?: string | null;
  reasoning_delta?: string | null;
  final_text?: string | null;
  tool_names?: string[] | null;
  terminal: boolean;
  emitted_at_utc: string;
}
