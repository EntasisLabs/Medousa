import type {
  ProfileShelfEntry,
  ProfileShelfFilter,
  ProfileShelfKind,
} from "$lib/types/profileShelf";
import type {
  IdentityContextResponse,
  IdentityRememberRequest,
  IdentityRelationship,
} from "$lib/types/identity";
import { preferenceDisplayValue } from "$lib/utils/identityTeach";

const KIND_LABELS: Record<ProfileShelfKind, string> = {
  claim: "Remembers",
  contact: "Person",
  relationship: "Connection",
  preference: "Preference",
};

export function profileKindLabel(kind: ProfileShelfKind): string {
  return KIND_LABELS[kind];
}

function relationshipKind(value: unknown): string {
  if (typeof value === "string") return value;
  if (value && typeof value === "object" && "type" in value) {
    return String((value as { type?: string }).type ?? "connection");
  }
  return "connection";
}

function humanizePrefKey(key: string): string {
  if (key.startsWith("note_")) {
    return "Remembers";
  }
  return key
    .split(/[_-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function contactIdForRelationship(rel: IdentityRelationship): string | null {
  if (rel.target_entity_ref?.entity_type === "ContactEntity") {
    return rel.target_entity_ref.entity_id;
  }
  return null;
}

function relationshipRole(rel: IdentityRelationship | undefined): string | null {
  if (!rel) return null;
  const reason = rel.last_transition_reason?.trim();
  if (reason) return reason;
  if (rel.policy_tags && rel.policy_tags.length > 0) {
    return rel.policy_tags.join(", ");
  }
  return null;
}

export function buildProfileShelfEntries(
  context: IdentityContextResponse,
): ProfileShelfEntry[] {
  const entries: ProfileShelfEntry[] = [];
  const relationshipsByContactId = new Map<string, IdentityRelationship>();

  for (const rel of context.relationships ?? []) {
    const contactId = contactIdForRelationship(rel);
    if (contactId) {
      relationshipsByContactId.set(contactId, rel);
    }
  }

  const contactIds = new Set((context.contacts ?? []).map((contact) => contact.contact_id));

  for (const contact of context.contacts ?? []) {
    const rel = relationshipsByContactId.get(contact.contact_id);
    const role = relationshipRole(rel);
    entries.push({
      id: `contact:${contact.contact_id}`,
      kind: "contact",
      title: contact.display_name,
      subtitle: role ?? "Someone she should recognize",
      searchText: [contact.display_name, role, contact.contact_id, "person"].filter(Boolean).join(" "),
      trustLevel: rel?.trust_level,
      confidence: rel?.confidence,
      rememberKind: "person",
      rememberSubject: contact.display_name,
      relationshipId: rel?.relationship_id,
      meta: {
        contact_id: contact.contact_id,
        ...(rel?.relationship_id ? { relationship_id: rel.relationship_id } : {}),
      },
    });
  }

  for (const claim of context.flattened_claims ?? []) {
    entries.push({
      id: `claim:${claim.claim_id}`,
      kind: "claim",
      title: claim.summary,
      subtitle: `She's ${(claim.confidence * 100).toFixed(0)}% sure about this`,
      searchText: [claim.summary, claim.claim_id, "remembers fact"].join(" "),
      confidence: claim.confidence,
      rememberKind: "note",
      rememberSubject: claim.summary.slice(0, 48),
      meta: { claim_id: claim.claim_id },
    });
  }

  const timezone =
    preferenceDisplayValue(context.user?.preferences?.timezone) ||
    context.user?.timezone?.trim() ||
    "";

  if (timezone) {
    entries.push({
      id: "pref:timezone",
      kind: "preference",
      title: "Timezone",
      subtitle: timezone,
      searchText: [timezone, "timezone"].join(" "),
      rememberKind: "preference",
      rememberSubject: "timezone",
      meta: { preference_key: "timezone" },
    });
  }

  if (context.user?.preferences) {
    for (const [key, value] of Object.entries(context.user.preferences)) {
      if (key === "timezone") continue;
      const display = preferenceDisplayValue(value);
      if (!display.trim()) continue;
      entries.push({
        id: `pref:${key}`,
        kind: key.startsWith("note_") ? "claim" : "preference",
        title: humanizePrefKey(key),
        subtitle: display,
        searchText: [key, display, humanizePrefKey(key), "preference", "note"].join(" "),
        rememberKind: "preference",
        rememberSubject: key,
        meta: { preference_key: key },
      });
    }
  }

  for (const rel of context.relationships ?? []) {
    const contactId = contactIdForRelationship(rel);
    if (contactId && contactIds.has(contactId)) continue;

    const role = relationshipRole(rel) ?? relationshipKind(rel.relationship_kind);
    entries.push({
      id: `relationship:${rel.relationship_id}`,
      kind: "relationship",
      title: role.charAt(0).toUpperCase() + role.slice(1),
      subtitle: `Trust ${(rel.trust_level * 100).toFixed(0)}% · ${(rel.confidence * 100).toFixed(0)}% confidence`,
      searchText: [role, rel.relationship_id, "relationship"].join(" "),
      trustLevel: rel.trust_level,
      confidence: rel.confidence,
      rememberKind: "note",
      rememberSubject: role,
      relationshipId: rel.relationship_id,
      meta: {
        relationship_id: rel.relationship_id,
        kind: relationshipKind(rel.relationship_kind),
      },
    });
  }

  const rank = (kind: ProfileShelfKind) => {
    if (kind === "contact") return 0;
    if (kind === "relationship") return 1;
    if (kind === "claim") return 2;
    if (kind === "preference") return 3;
    return 4;
  };

  return entries.sort((a, b) => rank(a.kind) - rank(b.kind) || a.title.localeCompare(b.title));
}

export function filterProfileShelfEntries(
  entries: ProfileShelfEntry[],
  query: string,
  tab: ProfileShelfFilter,
): ProfileShelfEntry[] {
  let filtered = entries;
  if (tab === "people") {
    filtered = filtered.filter(
      (entry) => entry.kind === "contact" || entry.kind === "relationship",
    );
  } else if (tab === "facts") {
    filtered = filtered.filter(
      (entry) => entry.kind === "claim" || entry.kind === "preference",
    );
  }

  const needle = query.trim().toLowerCase();
  if (!needle) return filtered;
  return filtered.filter((entry) => entry.searchText.toLowerCase().includes(needle));
}

export function shelfTabForRemember(factKind: string): ProfileShelfFilter {
  if (factKind === "person") return "people";
  return "facts";
}

export function findShelfEntryAfterRemember(
  entries: ProfileShelfEntry[],
  parsed: IdentityRememberRequest,
): ProfileShelfEntry | null {
  const subject = parsed.subject.trim().toLowerCase();
  const statement = parsed.statement.trim().toLowerCase();

  if (parsed.fact_kind === "person") {
    return (
      entries.find(
        (entry) =>
          entry.kind === "contact" && entry.title.trim().toLowerCase() === subject,
      ) ??
      entries.find(
        (entry) =>
          entry.kind === "relationship" &&
          entry.searchText.toLowerCase().includes(subject),
      ) ??
      null
    );
  }

  if (parsed.fact_kind === "preference") {
    return (
      entries.find(
        (entry) =>
          entry.kind === "preference" && entry.meta?.preference_key === parsed.subject,
      ) ?? null
    );
  }

  return (
    entries.find(
      (entry) =>
        entry.kind === "claim" &&
        (entry.subtitle.toLowerCase().includes(statement) ||
          entry.title.toLowerCase().includes(statement)),
    ) ??
    entries.find(
      (entry) =>
        entry.meta?.preference_key?.startsWith("note_") &&
        entry.subtitle.toLowerCase().includes(statement),
    ) ??
    null
  );
}

/** Strip engine tokens from digest text for human empty-state copy. */
export function humanDigestLines(text: string | null | undefined): string[] {
  if (!text?.trim()) return [];
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(
      (line) =>
        line.length > 0 &&
        !line.startsWith("[") &&
        !line.includes("status=empty") &&
        !line.startsWith("#") &&
        !line.toLowerCase().includes("medousa_relational"),
    );
}
