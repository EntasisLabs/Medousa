import type {
  ContextRecallEntry,
  ContextRecallKind,
} from "$lib/types/context";
import type { IdentityContextResponse } from "$lib/types/identity";

function relationshipKindSlug(value: unknown): string {
  if (typeof value === "string") return value.trim().toLowerCase();
  if (value && typeof value === "object") {
    if ("type" in value) {
      const typed = value as { type?: string; value?: string };
      if (typed.type === "Legacy" && typed.value?.trim()) {
        return typed.value.trim().toLowerCase();
      }
      if (typed.type?.trim()) return typed.type.trim().toLowerCase();
    }
  }
  return "knows";
}

function capitalizeWord(word: string): string {
  if (!word) return word;
  return word.charAt(0).toUpperCase() + word.slice(1);
}

function formatRelationshipKind(value: unknown): string {
  const slug = relationshipKindSlug(value);
  if (slug === "knows") return "Knows";
  return slug
    .split(/[_-]+/)
    .filter(Boolean)
    .map(capitalizeWord)
    .join(" ");
}

function formatPolicyTags(tags: string[] | undefined): string | null {
  if (!tags?.length) return null;
  const parts = tags
    .map((tag) => {
      const trimmed = tag.trim();
      if (!trimmed) return null;
      const colon = trimmed.indexOf(":");
      if (colon === -1) return trimmed;
      const prefix = trimmed.slice(0, colon).trim().toLowerCase();
      const value = trimmed.slice(colon + 1).trim();
      if (!value) return trimmed;
      if (prefix === "role") return value.replace(/_/g, " ");
      if (prefix === "employer") return `at ${value}`;
      return value;
    })
    .filter((part): part is string => Boolean(part?.trim()));
  return parts.length > 0 ? parts.join(" · ") : null;
}

function kindLabel(kind: ContextRecallKind): string {
  switch (kind) {
    case "claim":
      return "Claim";
    case "contact":
      return "Contact";
    case "relationship":
      return "Relationship";
    case "persona":
      return "Persona";
    case "user":
      return "You";
  }
}

export function recallKindLabel(kind: ContextRecallKind): string {
  return kindLabel(kind);
}

export function buildContextRecallEntries(
  context: IdentityContextResponse,
): ContextRecallEntry[] {
  const entries: ContextRecallEntry[] = [];

  if (context.persona) {
    entries.push({
      id: `persona:${context.persona.persona_id}`,
      kind: "persona",
      title: context.persona.display_name,
      subtitle: "Workshop persona",
      searchText: [
        context.persona.display_name,
        context.persona.persona_id,
        context.persona.status,
        "persona",
      ].join(" "),
      meta: {
        persona_id: context.persona.persona_id,
        status: context.persona.status,
      },
    });
  }

  if (context.user) {
    const preferenceKeys = context.user.preferences
      ? Object.keys(context.user.preferences)
      : [];
    entries.push({
      id: `user:${context.user.user_id}`,
      kind: "user",
      title: "You",
      subtitle:
        preferenceKeys.length > 0
          ? `${preferenceKeys.length} preference${preferenceKeys.length === 1 ? "" : "s"} she carries`
          : `${context.user.timezone} · operator profile`,
      searchText: [
        "you",
        context.user.user_id,
        context.user.timezone,
        context.user.status,
        ...preferenceKeys,
        "user profile",
      ].join(" "),
      meta: {
        user_id: context.user.user_id,
        timezone: context.user.timezone,
        status: context.user.status,
      },
    });
  }

  for (const claim of context.flattened_claims ?? []) {
    entries.push({
      id: `claim:${claim.claim_id}`,
      kind: "claim",
      title: claim.summary,
      subtitle: `${(claim.confidence * 100).toFixed(0)}% sure`,
      searchText: [claim.summary, claim.claim_id, "claim recall"].join(" "),
      confidence: claim.confidence,
      meta: {
        claim_id: claim.claim_id,
      },
    });
  }

  for (const contact of context.contacts ?? []) {
    entries.push({
      id: `contact:${contact.contact_id}`,
      kind: "contact",
      title: contact.display_name,
      subtitle: "Someone in your graph",
      searchText: [contact.display_name, contact.contact_id, "contact person"].join(
        " ",
      ),
      meta: {
        contact_id: contact.contact_id,
      },
    });
  }

  for (const rel of context.relationships ?? []) {
    const kind = formatRelationshipKind(rel.relationship_kind);
    const tags = formatPolicyTags(rel.policy_tags);
    const reason = rel.last_transition_reason?.trim() || "";
    const subtitleParts = [
      `${(rel.trust_level * 100).toFixed(0)}% trust`,
      tags,
    ].filter(Boolean);
    entries.push({
      id: `relationship:${rel.relationship_id}`,
      kind: "relationship",
      title: kind,
      subtitle: subtitleParts.join(" · "),
      searchText: [
        kind,
        rel.relationship_id,
        tags ?? "",
        reason,
        "relationship trust",
      ].join(" "),
      trustLevel: rel.trust_level,
      confidence: rel.confidence,
      meta: {
        relationship_id: rel.relationship_id,
        kind,
        ...(tags ? { policy_tags: tags } : {}),
        ...(reason ? { transition_reason: reason } : {}),
      },
    });
  }

  return entries;
}

export function filterContextRecallEntries(
  entries: ContextRecallEntry[],
  query: string,
): ContextRecallEntry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return entries;
  return entries.filter((entry) => entry.searchText.toLowerCase().includes(needle));
}
