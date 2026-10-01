export const BOT_AVATARS = [
  { id: "mascot:medousa", label: "Medousa", color: "#38bdf8", mascot: "medousa" },
  { id: "mascot:seahorse", label: "Seahorse", color: "#2dd4bf", mascot: "seahorse" },
  { id: "mascot:starfish", label: "Starfish", color: "#f5b841", mascot: "starfish" },
  { id: "medousa:violet", label: "Violet", color: "#b394f6", mascot: null },
  { id: "medousa:blue", label: "Ocean", color: "#77bafa", mascot: null },
  { id: "medousa:jade", label: "Jade", color: "#79d2b3", mascot: null },
  { id: "medousa:amber", label: "Amber", color: "#ebbf74", mascot: null },
  { id: "medousa:rose", label: "Rose", color: "#ea99c3", mascot: null },
  { id: "medousa:pearl", label: "Pearl", color: "#d7d9e8", mascot: null },
] as const;
export const DEFAULT_BOT_AVATAR = "medousa:violet";

export function botAvatar(value: string | null | undefined) {
  const ref = value?.trim() ?? "";
  const mark = BOT_AVATARS.find((entry) => entry.id === ref);
  // Keep existing emoji avatars readable; unrecognized references use the mark.
  const legacy = !mark && !ref.startsWith("medousa:") && !ref.startsWith("mascot:") && /\p{Extended_Pictographic}/u.test(ref)
    && [...ref].length <= 12 ? ref : null;
  return { ...(mark ?? BOT_AVATARS.find((entry) => entry.id === DEFAULT_BOT_AVATAR)!), legacy };
}
