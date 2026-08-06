/** Lucide icon ids allowed in Liquid shortcodes and fence fields. */
export const LIQUID_ICON_ALLOWLIST = new Set([
  "sparkles",
  "lock",
  "globe",
  "message-circle",
  "messagecircle",
  "brain",
  "shield",
  "code",
  "cpu",
  "zap",
  "clock",
  "hourglass",
  "coins",
  "tag",
  "mic",
  "pencil",
  "file-code",
  "filecode",
  "table",
  "layers",
  "rocket",
  "star",
  "check",
  "x",
  "info",
  "alert-triangle",
  "alerttriangle",
  "search",
  "book",
  "map",
  "compass",
  "plane",
  "map-pin",
  "mappin",
  "hotel",
  "camera",
  "heart",
  "home",
  "calendar",
  "sun",
  "moon",
  "coffee",
  "train",
  "train-front",
  "trainfront",
  "car",
  "building",
  "building-2",
  "building2",
  "landmark",
  "mountain",
  "utensils",
  "shopping-bag",
  "shoppingbag",
  "music",
  "users",
  "flag",
  "navigation",
  "house",
  "bed",
]);

/** Normalize a raw icon id to the canonical kebab form, or reject it. */
export function normalizeLiquidIconId(raw: string): string | null {
  const id = raw.trim().toLowerCase().replace(/_/g, "-");
  if (!id || !LIQUID_ICON_ALLOWLIST.has(id)) return null;
  return id
    .replace(/^messagecircle$/, "message-circle")
    .replace(/^filecode$/, "file-code")
    .replace(/^alerttriangle$/, "alert-triangle")
    .replace(/^mappin$/, "map-pin")
    .replace(/^building2$/, "building-2")
    .replace(/^shoppingbag$/, "shopping-bag")
    .replace(/^building$/, "building-2")
    .replace(/^trainfront$/, "train-front")
    .replace(/^home$/, "house");
}
