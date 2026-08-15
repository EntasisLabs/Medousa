import type {
  TurnStreamEnvelopeV2,
  TurnStreamEventV2,
} from "$lib/types/generated/daemon_api";

export type TerminalTurnStreamEventV2 = Extract<
  TurnStreamEventV2,
  {
    type:
      | "final"
      | "needs_input"
      | "checkpoint"
      | "worker_synthesis"
      | "error";
  }
>;

export function isTurnStreamEnvelopeV2(
  payload: unknown,
): payload is TurnStreamEnvelopeV2 {
  return (
    payload != null &&
    typeof payload === "object" &&
    "schema_version" in payload &&
    "event" in payload
  );
}

export function isTerminalTurnStreamEventV2(
  event: TurnStreamEventV2,
): event is TerminalTurnStreamEventV2 {
  return (
    event.type === "final" ||
    event.type === "needs_input" ||
    event.type === "checkpoint" ||
    event.type === "worker_synthesis" ||
    event.type === "error"
  );
}

export function terminalTurnStreamTextV2(
  event: TerminalTurnStreamEventV2,
): string {
  return event.type === "error" ? event.operator_message : event.text;
}
