export const BOT_AVATARS = [
  { id: "medousa:violet", label: "Violet", color: "#b394f6" },
  { id: "medousa:blue", label: "Ocean", color: "#77bafa" },
  { id: "medousa:jade", label: "Jade", color: "#79d2b3" },
  { id: "medousa:amber", label: "Amber", color: "#ebbf74" },
  { id: "medousa:rose", label: "Rose", color: "#ea99c3" },
  { id: "medousa:pearl", label: "Pearl", color: "#d7d9e8" },
] as const;
export const DEFAULT_BOT_AVATAR = BOT_AVATARS[0].id;

export function botAvatar(value: string | null | undefined) {
  const ref = value?.trim() ?? "";
  const mark = BOT_AVATARS.find((entry) => entry.id === ref);
  // Keep existing emoji avatars readable; unrecognized references use the mark.
  const legacy = !mark && !ref.startsWith("medousa:") && /\p{Extended_Pictographic}/u.test(ref)
    && [...ref].length <= 12 ? ref : null;
  return { ...(mark ?? BOT_AVATARS[0]), legacy };
}
