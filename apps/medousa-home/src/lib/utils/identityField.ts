import type { IdentityContextResponse } from "$lib/types/identity";
import type { IdentityFieldBlob, IdentityFieldLayout } from "$lib/types/identityField";
import { buildProfileShelfEntries } from "$lib/utils/profileShelf";

const FIELD_W = 1200;
const FIELD_H = 820;
const CX = FIELD_W / 2;
const CY = FIELD_H / 2;

const PERSON_FILLS = [
  "rgb(var(--color-primary-400) / 0.82)",
  "rgb(var(--color-secondary-400) / 0.78)",
  "rgb(var(--color-tertiary-400) / 0.75)",
  "rgb(var(--color-success-400) / 0.72)",
  "rgb(var(--color-warning-400) / 0.7)",
];

const PREF_FILL = "rgb(var(--color-surface-400) / 0.35)";

function clusterFill(density: number): string {
  const alpha = 0.55 + Math.min(density, 8) * 0.04;
  return `rgb(var(--color-primary-500) / ${alpha.toFixed(2)})`;
}

function synthesizePortrait(displayName: string, people: number, prefs: number): string {
  if (people === 0 && prefs === 0) {
    return `She's still learning who ${displayName} is — teach her something and watch the field take shape.`;
  }
  const parts: string[] = [];
  if (people > 0) {
    parts.push(
      people === 1 ? "someone close orbits you" : `${people} people pull on your center`,
    );
  }
  if (prefs > 0) {
    parts.push(prefs === 1 ? "a rhythm tints the air" : "rhythms tint how she shows up");
  }
  return `${displayName} — ${parts.join("; ")}.`;
}

export function buildIdentityFieldLayout(
  context: IdentityContextResponse | null,
  displayName: string,
  digestLines: string[] = [],
): IdentityFieldLayout {
  const entries = context ? buildProfileShelfEntries(context) : [];
  const people = entries.filter((e) => e.kind === "contact" || e.kind === "relationship");
  const preferences = entries.filter((e) => e.kind === "preference");
  const remembers = entries.filter((e) => e.kind === "claim");

  const density = people.length + remembers.length + preferences.length;
  const clusterRadius = 88 + Math.min(density * 10, 72);

  const portrait =
    digestLines.find((line) => line.length > 12 && line.length < 140) ??
    synthesizePortrait(displayName, people.length, preferences.length);

  const blobs: IdentityFieldBlob[] = [
    {
      id: "cluster:you",
      kind: "cluster",
      label: displayName,
      subtitle: portrait,
      x: CX,
      y: CY,
      radius: clusterRadius,
      opacity: 0.95,
      fill: clusterFill(density),
      entry: null,
    },
  ];

  const personCount = people.length;
  people.forEach((entry, index) => {
    const trust = entry.trustLevel ?? 0.65;
    const angle = (index / Math.max(personCount, 1)) * Math.PI * 2 - Math.PI / 2;
    const orbit = 168 + (1 - trust) * 56 + (index % 3) * 12;
    blobs.push({
      id: entry.id,
      kind: "person",
      label: entry.title,
      subtitle: entry.subtitle,
      x: CX + Math.cos(angle) * orbit,
      y: CY + Math.sin(angle) * orbit * 0.82,
      radius: 38 + trust * 22,
      opacity: 0.88,
      fill: PERSON_FILLS[index % PERSON_FILLS.length],
      entry,
    });
  });

  const prefCount = preferences.length;
  preferences.forEach((entry, index) => {
    const angle =
      (index / Math.max(prefCount, 1)) * Math.PI * 2 + Math.PI / prefCount + 0.4;
    const orbit = 300 + (index % 4) * 28;
    blobs.push({
      id: entry.id,
      kind: "preference",
      label: entry.title,
      subtitle: entry.subtitle,
      x: CX + Math.cos(angle) * orbit,
      y: CY + Math.sin(angle) * orbit * 0.75,
      radius: 52 + (index % 2) * 14,
      opacity: 0.42,
      fill: PREF_FILL,
      entry,
    });
  });

  // Relational density: small inner satellites for remembered claims
  remembers.slice(0, 6).forEach((entry, index) => {
    const angle = 0.8 + index * 1.15;
    const orbit = clusterRadius * 0.55 + index * 8;
    blobs.push({
      id: entry.id,
      kind: "cluster",
      label: entry.title.slice(0, 32),
      subtitle: entry.subtitle,
      x: CX + Math.cos(angle) * orbit,
      y: CY + Math.sin(angle) * orbit,
      radius: 22 + (entry.confidence ?? 0.5) * 14,
      opacity: 0.5,
      fill: "rgb(var(--color-primary-300) / 0.45)",
      entry,
    });
  });

  return {
    width: FIELD_W,
    height: FIELD_H,
    centerX: CX,
    centerY: CY,
    portrait,
    blobs,
  };
}

/** Fit field bounds with padding for initial viewport. */
export function fieldViewportForBlobs(
  layout: IdentityFieldLayout,
  viewW: number,
  viewH: number,
  pad = 48,
): { scale: number; offsetX: number; offsetY: number } {
  if (layout.blobs.length === 0) {
    return { scale: 1, offsetX: 0, offsetY: 0 };
  }
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const blob of layout.blobs) {
    minX = Math.min(minX, blob.x - blob.radius);
    minY = Math.min(minY, blob.y - blob.radius);
    maxX = Math.max(maxX, blob.x + blob.radius);
    maxY = Math.max(maxY, blob.y + blob.radius);
  }
  const boxW = maxX - minX + pad * 2;
  const boxH = maxY - minY + pad * 2;
  const scale = Math.min(viewW / boxW, viewH / boxH, 1.15);
  const offsetX = viewW / 2 - ((minX + maxX) / 2) * scale;
  const offsetY = viewH / 2 - ((minY + maxY) / 2) * scale;
  return { scale, offsetX, offsetY };
}
