import {
  BookOpen,
  Bug,
  Folder,
  Inbox,
  Layers,
  Settings,
  Wallet,
} from "@lucide/svelte";
import type { Component } from "svelte";

export function iconForSpace(spaceId: string | null | undefined): Component {
  switch (spaceId) {
    case "journal":
      return BookOpen;
    case "projects":
      return Folder;
    case "finance":
      return Wallet;
    case "inbox":
      return Inbox;
    case "bugs":
      return Bug;
    case "system_bucket":
      return Settings;
    case "other":
      return Layers;
    default:
      if (spaceId?.startsWith("custom_")) return Folder;
      return Layers;
  }
}
