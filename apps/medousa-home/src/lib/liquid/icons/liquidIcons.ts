/**
 * Shared Lucide allowlist + component map for `{{icon:name}}` and fence `icon:` fields.
 */
import {
  AlertTriangle,
  Book,
  Bed,
  Brain,
  Building2,
  Calendar,
  Camera,
  Car,
  Check,
  Clock,
  Code,
  Coffee,
  Coins,
  Compass,
  Cpu,
  FileCode,
  Flag,
  Globe,
  Heart,
  Hotel,
  Hourglass,
  House,
  Info,
  Landmark,
  Layers,
  Lock,
  Map,
  MapPin,
  MessageCircle,
  Mic,
  Moon,
  Mountain,
  Music,
  Navigation,
  Pencil,
  Plane,
  Rocket,
  Search,
  Shield,
  ShoppingBag,
  Sparkles,
  Star,
  Sun,
  Table,
  Tag,
  TrainFront,
  Utensils,
  Users,
  X,
  Zap,
  type Icon as LucideIcon,
} from "@lucide/svelte";

/** Lucide icon ids allowed in `{{icon:name}}` and fence `icon:` (kebab or camel). */
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
  // Timeline / travel / everyday glyphs
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

export const LIQUID_ICON_MAP: Record<string, typeof LucideIcon> = {
  sparkles: Sparkles,
  lock: Lock,
  globe: Globe,
  "message-circle": MessageCircle,
  brain: Brain,
  shield: Shield,
  code: Code,
  cpu: Cpu,
  zap: Zap,
  clock: Clock,
  hourglass: Hourglass,
  coins: Coins,
  tag: Tag,
  mic: Mic,
  pencil: Pencil,
  "file-code": FileCode,
  table: Table,
  layers: Layers,
  rocket: Rocket,
  star: Star,
  check: Check,
  x: X,
  info: Info,
  "alert-triangle": AlertTriangle,
  search: Search,
  book: Book,
  map: Map,
  compass: Compass,
  plane: Plane,
  "map-pin": MapPin,
  hotel: Hotel,
  camera: Camera,
  heart: Heart,
  home: House,
  house: House,
  calendar: Calendar,
  sun: Sun,
  moon: Moon,
  coffee: Coffee,
  train: TrainFront,
  "train-front": TrainFront,
  car: Car,
  building: Building2,
  "building-2": Building2,
  landmark: Landmark,
  mountain: Mountain,
  utensils: Utensils,
  "shopping-bag": ShoppingBag,
  music: Music,
  users: Users,
  flag: Flag,
  navigation: Navigation,
  bed: Bed,
};

/** Normalize raw fence / shortcode id to canonical kebab form, or null if not allowed. */
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

export function liquidIconComponent(raw: string | null | undefined): typeof LucideIcon | null {
  if (!raw) return null;
  const id = normalizeLiquidIconId(raw);
  if (!id) return null;
  return LIQUID_ICON_MAP[id] ?? null;
}

/** True when the string looks like a Lucide id rather than an emoji/glyph. */
export function looksLikeLiquidIconId(raw: string): boolean {
  const t = raw.trim();
  if (!t || /\s/.test(t)) return false;
  // Emoji / symbol — not an icon id
  if (/[^\x00-\x7F]/.test(t) || /[\uFE0F\u200D]/.test(t)) return false;
  return /^[a-zA-Z][a-zA-Z0-9_-]*$/.test(t);
}

/**
 * Resolve a glyph: prefer explicit `icon`, else treat `emoji` as Lucide id when it matches.
 */
export function resolveLiquidGlyph(options: {
  icon?: string | null;
  emoji?: string | null;
}): { kind: "icon"; id: string; component: typeof LucideIcon } | { kind: "text"; text: string } | null {
  const iconRaw = options.icon?.trim();
  if (iconRaw) {
    const id = normalizeLiquidIconId(iconRaw);
    const component = id ? LIQUID_ICON_MAP[id] : null;
    if (id && component) return { kind: "icon", id, component };
  }
  const emojiRaw = options.emoji?.trim();
  if (!emojiRaw) return null;
  if (looksLikeLiquidIconId(emojiRaw)) {
    const id = normalizeLiquidIconId(emojiRaw);
    const component = id ? LIQUID_ICON_MAP[id] : null;
    if (id && component) return { kind: "icon", id, component };
  }
  return { kind: "text", text: emojiRaw };
}
