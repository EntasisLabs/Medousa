import type { Component } from "svelte";
import {
  Brain,
  Circle,
  MessageCircle,
  MessagesSquare,
  Monitor,
  Phone,
  Plug,
  Server,
  Sparkles,
  Terminal,
  Zap,
  type Icon as LucideIcon,
} from "@lucide/svelte";

const icons: Record<string, typeof LucideIcon> = {
  Zap,
  Brain,
  Terminal,
  Server,
  Monitor,
  MessageCircle,
  MessagesSquare,
  Phone,
  Plug,
  Sparkles,
  Circle,
};

export function resolveIcon(name: string): typeof LucideIcon {
  return icons[name] ?? Circle;
}

export type IconComponent = Component;
