import type { EnvironmentSpec } from "$lib/types/environment";

/** Shared daemon contract: no per-device Bot pin preferences. */
export function setBotNavigationPinned(spec: EnvironmentSpec, botId: string, pinned: boolean): void {
  if (!botId || botId.trim() !== botId || botId.length > 256 || /[\x00-\x1f\x7f]/.test(botId)) {
    throw new Error("Invalid Bot ID.");
  }
  const nav = spec.nav ?? { botPins: [], botAccents: {} };
  const pins = nav.botPins ?? [];
  const next = pinned ? [...new Set([...pins, botId])] : pins.filter((id) => id !== botId);
  if (next.length > 256) throw new Error("Too many pinned Bots.");
  spec.nav = { ...nav, botPins: next };
}

export function botNavigationAccent(spec: EnvironmentSpec | null, botId: string): string | undefined {
  const color = spec?.nav?.botAccents?.[botId];
  return color && /^#[0-9a-f]{6}$/i.test(color) ? color : undefined;
}
