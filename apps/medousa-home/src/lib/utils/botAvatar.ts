import type { MascotBody, MascotExpression } from "$lib/theme/medousaMarks";

export const BOT_MASCOT_BODIES = [
  { id: "medousa", label: "Medousa", color: "#38bdf8" },
  { id: "seahorse", label: "Seahorse", color: "#2dd4bf" },
  { id: "starfish", label: "Starfish", color: "#f5b841" },
] as const satisfies ReadonlyArray<{ id: MascotBody; label: string; color: string }>;

export const BOT_MASCOT_EXPRESSIONS = [
  { id: "default", label: "Default" },
  { id: "happy", label: "Happy" },
  { id: "chill", label: "Chill" },
  { id: "sweet", label: "Sweet" },
  { id: "focus", label: "Focus" },
  { id: "sus", label: "Sus" },
] as const satisfies ReadonlyArray<{ id: MascotExpression; label: string }>;

export const BOT_AVATARS = [
  ...BOT_MASCOT_BODIES.flatMap((body) => BOT_MASCOT_EXPRESSIONS.map((expression) => ({
    id: expression.id === "default" ? `mascot:${body.id}` : `mascot:${body.id}:${expression.id}`,
    label: `${body.label} ${expression.label}`,
    color: body.color,
    mascot: body.id,
    expression: expression.id,
  }))),
  { id: "medousa:violet", label: "Violet", color: "#b394f6", mascot: null, expression: null },
  { id: "medousa:blue", label: "Ocean", color: "#77bafa", mascot: null, expression: null },
  { id: "medousa:jade", label: "Jade", color: "#79d2b3", mascot: null, expression: null },
  { id: "medousa:amber", label: "Amber", color: "#ebbf74", mascot: null, expression: null },
  { id: "medousa:rose", label: "Rose", color: "#ea99c3", mascot: null, expression: null },
  { id: "medousa:pearl", label: "Pearl", color: "#d7d9e8", mascot: null, expression: null },
];
export const DEFAULT_BOT_AVATAR = "medousa:violet";

export function botAvatar(value: string | null | undefined) {
  const ref = value?.trim() ?? "";
  const mark = BOT_AVATARS.find((entry) => entry.id === ref);
  // Keep existing emoji avatars readable; unrecognized references use the mark.
  const legacy = !mark && !ref.startsWith("medousa:") && !ref.startsWith("mascot:") && /\p{Extended_Pictographic}/u.test(ref)
    && [...ref].length <= 12 ? ref : null;
  return { ...(mark ?? BOT_AVATARS.find((entry) => entry.id === DEFAULT_BOT_AVATAR)!), legacy };
}
