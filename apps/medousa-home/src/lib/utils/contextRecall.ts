import type {
  ContextRecallEntry,
  ContextRecallKind,
} from "$lib/types/context";
import type { IdentityContextResponse } from "$lib/types/identity";

function relationshipKind(value: unknown): string {
  if (typeof value === "string") return value;
  if (value && typeof value === "object" && "type" in value) {
    return String((value as { type?: string }).type ?? "relationship");
  }
  return "relationship";
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
      subtitle: `${context.persona.status} · workshop persona`,
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
      title: context.user.user_id,
      subtitle:
        preferenceKeys.length > 0
          ? `${context.user.timezone} · ${preferenceKeys.length} preference${preferenceKeys.length === 1 ? "" : "s"}`
          : `${context.user.timezone} · operator profile`,
      searchText: [
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
      subtitle: `Recall claim · ${(claim.confidence * 100).toFixed(0)}% confidence`,
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
      subtitle: "Person in your graph",
      searchText: [contact.display_name, contact.contact_id, "contact person"].join(
        " ",
      ),
      meta: {
        contact_id: contact.contact_id,
      },
    });
  }

  for (const rel of context.relationships ?? []) {
    const kind = relationshipKind(rel.relationship_kind);
    entries.push({
      id: `relationship:${rel.relationship_id}`,
      kind: "relationship",
      title: kind,
      subtitle: `Trust ${(rel.trust_level * 100).toFixed(0)}% · ${(rel.confidence * 100).toFixed(0)}% confidence`,
      searchText: [
        kind,
        rel.relationship_id,
        "relationship trust",
      ].join(" "),
      trustLevel: rel.trust_level,
      confidence: rel.confidence,
      meta: {
        relationship_id: rel.relationship_id,
        kind,
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
