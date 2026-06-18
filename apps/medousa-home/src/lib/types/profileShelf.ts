export type ProfileShelfKind =
  | "claim"
  | "contact"
  | "relationship"
  | "preference";

export type ProfileShelfFilter = "all" | "people" | "facts";

export interface ProfileShelfEntry {
  id: string;
  kind: ProfileShelfKind;
  title: string;
  subtitle: string;
  searchText: string;
  confidence?: number;
  trustLevel?: number;
  /** For corrections via identity remember */
  rememberSubject?: string;
  rememberKind?: "preference" | "person" | "note";
  relationshipId?: string;
  meta?: Record<string, string>;
}
