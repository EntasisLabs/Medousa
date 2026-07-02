import {
  Activity,
  BookOpen,
  Calendar,
  Globe,
  Home,
  LayoutGrid,
  MessageCircle,
  Orbit,
  PenLine,
  Radio,
  Settings,
  Sparkles,
  Zap,
  type Icon,
} from "@lucide/svelte";

const ICONS: Record<string, typeof Icon> = {
  home: Home,
  activity: Activity,
  "message-circle": MessageCircle,
  "layout-grid": LayoutGrid,
  "book-open": BookOpen,
  globe: Globe,
  orbit: Orbit,
  zap: Zap,
  calendar: Calendar,
  radio: Radio,
  settings: Settings,
  "pen-line": PenLine,
  sparkles: Sparkles,
};

export function environmentIcon(name: string): typeof Icon {
  return ICONS[name] ?? Sparkles;
}
