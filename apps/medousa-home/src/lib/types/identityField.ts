import type { ProfileShelfEntry } from "$lib/types/profileShelf";

export type IdentityBlobKind = "cluster" | "person" | "preference";

export interface IdentityFieldBlob {
  id: string;
  kind: IdentityBlobKind;
  label: string;
  subtitle: string;
  x: number;
  y: number;
  radius: number;
  opacity: number;
  /** CSS color (rgb/rgba) */
  fill: string;
  entry: ProfileShelfEntry | null;
}

export interface IdentityFieldLayout {
  width: number;
  height: number;
  centerX: number;
  centerY: number;
  portrait: string;
  blobs: IdentityFieldBlob[];
}
