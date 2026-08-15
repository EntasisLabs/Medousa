import {
  createTurnStreamProjectionState,
  projectTurnStreamEvent,
  type TurnStreamEnvelopeV2,
  type TurnStreamProjectedEvent,
  type TurnStreamProjectionState,
} from "@medousa/client";

export type ProjectedEvent = TurnStreamProjectedEvent;
export type ProjectionState = TurnStreamProjectionState;

export function createProjectionState(showEngineDetails = false): ProjectionState {
  return createTurnStreamProjectionState(showEngineDetails);
}

export function projectStreamEvent(
  event: TurnStreamEnvelopeV2,
  state: ProjectionState,
): ProjectedEvent[] {
  return projectTurnStreamEvent(event, state);
}
