export interface CarPlayLiveAction { owner: string; action: "start" | "mute" | "unmute" | "stop" }

export function permittedCarPlayAction(value: unknown, owner: string, active: boolean, busy: boolean): CarPlayLiveAction | null {
  if (busy || !value || typeof value !== "object") return null;
  const candidate = value as Partial<CarPlayLiveAction>;
  if (candidate.owner !== owner) return null;
  if (candidate.action === "start" ? active : !active) return null;
  if (!["start", "mute", "unmute", "stop"].includes(candidate.action ?? "")) return null;
  return candidate as CarPlayLiveAction;
}
