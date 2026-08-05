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
import {
  LIQUID_ICON_ALLOWLIST,
  normalizeLiquidIconId,
} from "@medousa/liquid-markdown";

export { LIQUID_ICON_ALLOWLIST, normalizeLiquidIconId };

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
