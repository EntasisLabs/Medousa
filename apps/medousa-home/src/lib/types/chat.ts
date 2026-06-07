export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  streaming?: boolean;
  /** Latest daemon turn phase whisper (e.g. tool_loop, synthesis). */
  phase?: string | null;
  /** Human status line from stream event.message. */
  statusLine?: string | null;
  /** Tools invoked this turn (cumulative). */
  tools?: string[];
  /** Collapsed reasoning scratch from reasoning_delta. */
  reasoning?: string | null;
  /** Verifier / turn lifecycle state from session history. */
  answerState?: string | null;
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
