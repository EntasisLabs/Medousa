import type { LocusAvecSnapshot, LocusNodeSummary } from "$lib/types/locus";
import type { MapVaultNote } from "$lib/utils/contextMapNotes";
import { humanMomentTitle, sessionMapLabel } from "$lib/utils/contextHuman";
import {
  buildNoteGraphSlice,
  sessionIdForNoteChatTag,
} from "$lib/utils/contextMapNotes";
import { AVEC_DIMENSIONS } from "$lib/utils/contextPosture";

export type MapAvecMins = {
  stability: number;
  friction: number;
  logic: number;
  autonomy: number;
};

export function avecMinsActive(mins?: MapAvecMins | null): boolean {
  if (!mins) return false;
  return (
    mins.stability > 0 ||
    mins.friction > 0 ||
    mins.logic > 0 ||
    mins.autonomy > 0
  );
}

/** Moment passes when every dim with min > 0 has user_avec[dim] >= min. */
export function momentPassesAvecMins(
  thread: { user_avec?: LocusAvecSnapshot | null },
  mins?: MapAvecMins | null,
): boolean {
  if (!avecMinsActive(mins) || !mins) return true;
  const avec = thread.user_avec;
  if (!avec) return false;
  for (const dim of AVEC_DIMENSIONS) {
    const min = mins[dim.key];
    if (min > 0 && avec[dim.key] < min) return false;
  }
  return true;
}

export type ContextMapNodeKind = "session" | "thread" | "claim" | "note";

export type ContextMapEdgeKind =
  | "membership"
  | "sequence"
  | "session_chain"
  | "note_session"
  | "note_link"
  | "note_tag";

export type ContextMapRenderMode = "full" | "ghost";

export interface ContextMapNode {
  id: string;
  kind: ContextMapNodeKind;
  label: string;
  sessionId: string;
  syncKey?: string;
  /** Vault-relative path when kind === "note". */
  notePath?: string;
  x: number;
  y: number;
  radius: number;
  weight: number;
  hue: number;
  visible: boolean;
  expanded?: boolean;
  momentCount?: number;
  showLabel?: boolean;
  renderMode?: ContextMapRenderMode;
}

export interface ContextMapEdge {
  id: string;
  from: string;
  to: string;
  kind: ContextMapEdgeKind;
  visible: boolean;
  strength?: number;
  renderMode?: ContextMapRenderMode;
}

export interface ContextMapGraph {
  nodes: ContextMapNode[];
  edges: ContextMapEdge[];
  sessionCount: number;
  momentCount: number;
  width: number;
  height: number;
}

export interface ContextMapBounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

const MAX_SESSIONS = 120;
const MAX_THREADS_PER_SESSION = 30;
const MAX_GHOST_MOMENTS = 12;
const DEFAULT_AUTO_EXPAND = 5;
/** Link sessions that share a recent time neighborhood (Obsidian-like). */
const SESSION_PROXIMITY_MS = 7 * 24 * 60 * 60 * 1000;
const MAX_PROXIMITY_LINKS_PER_SESSION = 3;

export type ContextMapDensity = "default" | "rail";

const TIER_WEIGHT: Record<string, number> = {
  raw: 1,
  daily: 1.15,
  weekly: 1.3,
  monthly: 1.45,
  quarterly: 1.6,
  yearly: 1.75,
};

function parseTimestamp(value: string): number {
  const ms = Date.parse(value);
  return Number.isNaN(ms) ? 0 : ms;
}

function truncateLabel(text: string, max = 28): string {
  const trimmed = text.trim();
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max - 1)}…`;
}

function sessionHue(sessionId: string): number {
  let hash = 0;
  for (let i = 0; i < sessionId.length; i += 1) {
    hash = (hash * 31 + sessionId.charCodeAt(i)) >>> 0;
  }
  return hash % 8;
}

function sessionRadius(momentCount: number, density: ContextMapDensity): number {
  const base = Math.min(26, Math.max(9, 7 + Math.sqrt(Math.max(momentCount, 1)) * 3.2));
  return density === "rail" ? base * 0.82 : base;
}

function threadRadius(thread: LocusNodeSummary, density: ContextMapDensity): number {
  const tier = TIER_WEIGHT[thread.tier.trim().toLowerCase()] ?? 1;
  const signal = Math.min(1.4, 0.85 + thread.rho * 0.35 + thread.kappa * 0.2);
  const base = Math.min(11, Math.max(4.5, 4 + tier * 2.2 * signal));
  return density === "rail" ? base * 0.85 : base;
}

function threadWeight(thread: LocusNodeSummary): number {
  const tier = TIER_WEIGHT[thread.tier.trim().toLowerCase()] ?? 1;
  return tier * (0.75 + thread.rho * 0.5);
}

interface SessionBucket {
  sessionId: string;
  label: string;
  threads: LocusNodeSummary[];
}

function buildSessionBuckets(
  locusNodes: LocusNodeSummary[],
  sessionLabels: Record<string, string>,
): SessionBucket[] {
  const bySession = new Map<string, LocusNodeSummary[]>();
  for (const node of locusNodes) {
    const bucket = bySession.get(node.session_id) ?? [];
    bucket.push(node);
    bySession.set(node.session_id, bucket);
  }

  return [...bySession.entries()]
    .map(([sessionId, nodes]) => ({
      sessionId,
      label: sessionMapLabel(sessionId, sessionLabels, nodes[0]?.timestamp),
      threads: [...nodes].sort(
        (left, right) => parseTimestamp(right.timestamp) - parseTimestamp(left.timestamp),
      ),
    }))
    .sort(
      (left, right) =>
        parseTimestamp(right.threads[0]?.timestamp ?? "") -
        parseTimestamp(left.threads[0]?.timestamp ?? ""),
    )
    .slice(0, MAX_SESSIONS);
}

/** Cheap ring seed only — settle physics owns the real layout. */
function seedNodePositions(
  nodes: ContextMapNode[],
  width: number,
  height: number,
  priorPositions?: Map<string, { x: number; y: number }>,
): void {
  const visibleNodes = nodes.filter((node) => node.visible);
  if (visibleNodes.length === 0) return;

  const cx = width / 2;
  const cy = height / 2;
  const bySession = new Map<string, { x: number; y: number }>();

  // Sessions first so moments/notes can seed nearby.
  const ordered = [
    ...visibleNodes.filter((node) => node.kind === "session"),
    ...visibleNodes.filter((node) => node.kind !== "session"),
  ];

  ordered.forEach((node, index) => {
    const prior = priorPositions?.get(node.id);
    if (prior) {
      node.x = prior.x;
      node.y = prior.y;
      if (node.kind === "session") {
        bySession.set(node.sessionId, { x: node.x, y: node.y });
      }
      return;
    }

    if (node.kind === "session") {
      const angle = (Math.PI * 2 * index) / Math.max(visibleNodes.length, 1) - Math.PI / 2;
      const spread = Math.min(width, height) * 0.38;
      node.x = cx + Math.cos(angle) * spread * (0.75 + (node.weight / 12) * 0.35);
      node.y = cy + Math.sin(angle) * spread * (0.75 + (node.weight / 12) * 0.35);
      bySession.set(node.sessionId, { x: node.x, y: node.y });
      return;
    }

    const parent = node.sessionId ? bySession.get(node.sessionId) : undefined;
    if (parent) {
      const angle = index * 2.399 + (node.kind === "note" ? 0.7 : 0);
      const radius = (node.kind === "note" ? 48 : 30) + (index % 6) * 7;
      node.x = parent.x + Math.cos(angle) * radius;
      node.y = parent.y + Math.sin(angle) * radius;
      return;
    }

    const angle = (Math.PI * 2 * index) / Math.max(visibleNodes.length, 1) - Math.PI / 2;
    const spread = Math.min(width, height) * 0.38;
    node.x = cx + Math.cos(angle) * spread * 0.8;
    node.y = cy + Math.sin(angle) * spread * 0.8;
  });
}

export function graphBounds(graph: ContextMapGraph): ContextMapBounds | null {
  const visible = graph.nodes.filter((node) => node.visible);
  if (visible.length === 0) return null;

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const node of visible) {
    const pad = node.radius + 24;
    minX = Math.min(minX, node.x - pad);
    minY = Math.min(minY, node.y - pad);
    maxX = Math.max(maxX, node.x + pad);
    maxY = Math.max(maxY, node.y + pad);
  }

  return { minX, minY, maxX, maxY };
}

export function boundsForNodeIds(
  graph: ContextMapGraph,
  nodeIds: Set<string>,
): ContextMapBounds | null {
  const visible = graph.nodes.filter((node) => node.visible && nodeIds.has(node.id));
  if (visible.length === 0) return null;

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const node of visible) {
    const pad = node.radius + 28;
    minX = Math.min(minX, node.x - pad);
    minY = Math.min(minY, node.y - pad);
    maxX = Math.max(maxX, node.x + pad);
    maxY = Math.max(maxY, node.y + pad);
  }

  return { minX, minY, maxX, maxY };
}

export function mapNeighborhood(graph: ContextMapGraph, nodeId: string | null): Set<string> {
  if (!nodeId) return new Set();
  const ids = new Set<string>([nodeId]);
  const node = graph.nodes.find((entry) => entry.id === nodeId);
  if (!node) return ids;

  if (node.kind === "session") {
    for (const entry of graph.nodes) {
      if (entry.sessionId === node.sessionId) ids.add(entry.id);
    }
  }

  for (const edge of graph.edges) {
    if (!edge.visible) continue;
    if (edge.from === nodeId) ids.add(edge.to);
    if (edge.to === nodeId) ids.add(edge.from);
  }

  if (node.kind === "thread") {
    ids.add(`session:${node.sessionId}`);
  }

  return ids;
}

export function defaultExpandedSessionIds(
  locusNodes: LocusNodeSummary[],
  count = DEFAULT_AUTO_EXPAND,
): Set<string> {
  const bySession = new Map<string, number>();
  for (const node of locusNodes) {
    const ts = parseTimestamp(node.timestamp);
    const prev = bySession.get(node.session_id) ?? 0;
    if (ts > prev) bySession.set(node.session_id, ts);
  }

  return new Set(
    [...bySession.entries()]
      .sort((left, right) => right[1] - left[1])
      .slice(0, count)
      .map(([sessionId]) => sessionId),
  );
}

export function neighborSummary(graph: ContextMapGraph, nodeId: string): string {
  const node = graph.nodes.find((entry) => entry.id === nodeId);
  if (!node) return "";
  const neighborhood = mapNeighborhood(graph, nodeId);
  const moments = [...neighborhood].filter((id) => id.startsWith("thread:")).length;
  const sessions = [...neighborhood].filter((id) => id.startsWith("session:")).length;
  const notes = [...neighborhood].filter((id) => id.startsWith("note:")).length;

  if (node.kind === "session") {
    const total = node.momentCount ?? moments;
    const noteBit =
      notes > 0 ? ` · ${notes} note${notes === 1 ? "" : "s"}` : "";
    return `${total} moment${total === 1 ? "" : "s"} in this session${noteBit}`;
  }
  if (node.kind === "note") {
    return `${notes - 1 > 0 ? `${notes - 1} linked note${notes - 1 === 1 ? "" : "s"}` : "Vault note"}${
      sessions > 0 ? ` · ${sessions} session${sessions === 1 ? "" : "s"}` : ""
    }`;
  }
  return `${moments} linked moment${moments === 1 ? "" : "s"} · ${sessions} session${sessions === 1 ? "" : "s"}`;
}

function buildSessionProximityEdges(
  sessionNodeIds: string[],
  sessionTimestamps: Map<string, number>,
): ContextMapEdge[] {
  const sessions = sessionNodeIds
    .map((id) => ({ id, ts: sessionTimestamps.get(id) ?? 0 }))
    .sort((left, right) => right.ts - left.ts);

  const edges: ContextMapEdge[] = [];
  const seen = new Set<string>();

  for (let i = 0; i < sessions.length; i += 1) {
    const candidates: Array<{ j: number; dt: number }> = [];
    for (let j = 0; j < sessions.length; j += 1) {
      if (i === j) continue;
      const dt = Math.abs(sessions[i].ts - sessions[j].ts);
      if (dt <= SESSION_PROXIMITY_MS) {
        candidates.push({ j, dt });
      }
    }
    candidates.sort((left, right) => left.dt - right.dt);
    const picks = candidates.slice(0, MAX_PROXIMITY_LINKS_PER_SESSION);
    // Sparse fallback: keep a chronological neighbor so the graph doesn't fragment.
    if (picks.length === 0 && i < sessions.length - 1) {
      picks.push({
        j: i + 1,
        dt: Math.abs(sessions[i].ts - sessions[i + 1].ts),
      });
    }

    for (const { j, dt } of picks) {
      const from = sessions[i].id;
      const to = sessions[j].id;
      const key = from < to ? `${from}|${to}` : `${to}|${from}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const closeness =
        SESSION_PROXIMITY_MS > 0
          ? 1 - Math.min(dt, SESSION_PROXIMITY_MS) / SESSION_PROXIMITY_MS
          : 0.5;
      edges.push({
        id: `session_chain:${key}`,
        from,
        to,
        kind: "session_chain",
        visible: true,
        strength: 0.032 + closeness * 0.04,
      });
    }
  }

  return edges;
}

export function applySimulationPositions(
  graph: ContextMapGraph,
  positions: Map<string, { x: number; y: number }>,
): ContextMapGraph {
  if (positions.size === 0) return graph;
  return {
    ...graph,
    nodes: graph.nodes.map((node) => {
      const pos = positions.get(node.id);
      return pos ? { ...node, x: pos.x, y: pos.y } : node;
    }),
  };
}

/** @deprecated Prefer applySimulationPositions — kept for callers during migration. */
export function applyPinnedPositions(
  graph: ContextMapGraph,
  pins: Map<string, { x: number; y: number }>,
): ContextMapGraph {
  return applySimulationPositions(graph, pins);
}

export function buildContextMapGraph(
  locusNodes: LocusNodeSummary[],
  sessionLabels: Record<string, string>,
  options: {
    width: number;
    height: number;
    expandedSessionIds: Set<string>;
    searchQuery?: string;
    density?: ContextMapDensity;
    /** Preserve settled coords across expand/search rebuilds. */
    priorPositions?: Map<string, { x: number; y: number }>;
    vaultNotes?: MapVaultNote[];
    /** Per-dimension AVEC minimums (0 = off). */
    avecMins?: MapAvecMins | null;
  },
): ContextMapGraph {
  const {
    width,
    height,
    expandedSessionIds,
    searchQuery = "",
    density = "default",
    priorPositions,
    vaultNotes = [],
    avecMins = null,
  } = options;
  const needle = searchQuery.trim().toLowerCase();
  const avecFilterOn = avecMinsActive(avecMins);
  const buckets = buildSessionBuckets(locusNodes, sessionLabels);
  const allBucketSessionIds = buckets.map((bucket) => bucket.sessionId);
  const noteRevealSessions = new Set<string>();
  if (needle && vaultNotes.length > 0) {
    for (const note of vaultNotes) {
      const tags = note.tags ?? [];
      const hay = [note.title, note.path, ...tags].join(" ").toLowerCase();
      if (!hay.includes(needle)) continue;
      const linked = sessionIdForNoteChatTag(tags, allBucketSessionIds);
      if (linked) noteRevealSessions.add(linked);
    }
  }

  const floorW = density === "rail" ? 280 : 720;
  const floorH = density === "rail" ? 360 : 520;
  const growW = density === "rail" ? 28 : 52;
  const growH = density === "rail" ? 20 : 34;
  const layoutWidth = Math.max(width, floorW + buckets.length * growW);
  const layoutHeight = Math.max(height, floorH + buckets.length * growH);

  const nodes: ContextMapNode[] = [];
  const edges: ContextMapEdge[] = [];
  const sessionNodeIds: string[] = [];
  const sessionTimestamps = new Map<string, number>();

  for (const bucket of buckets) {
    const sessionMatches =
      !needle ||
      bucket.label.toLowerCase().includes(needle) ||
      bucket.sessionId.toLowerCase().includes(needle);
    const avecThreads = bucket.threads.filter((thread) =>
      momentPassesAvecMins(thread, avecMins),
    );
    if (avecFilterOn && avecThreads.length === 0) continue;

    const visibleThreads = avecThreads.slice(0, MAX_THREADS_PER_SESSION);
    const expanded = expandedSessionIds.has(bucket.sessionId);

    const matchingThreads = visibleThreads.filter((thread) => {
      if (!needle) return true;
      const title = humanMomentTitle(thread).toLowerCase();
      return (
        title.includes(needle) ||
        bucket.label.toLowerCase().includes(needle) ||
        bucket.sessionId.toLowerCase().includes(needle)
      );
    });

    const searchReveal = Boolean(needle && matchingThreads.length > 0);
    const showMomentsFull = expanded || searchReveal;

    const showSession =
      !needle ||
      sessionMatches ||
      matchingThreads.length > 0 ||
      noteRevealSessions.has(bucket.sessionId);
    if (!showSession) continue;

    const sessionId = `session:${bucket.sessionId}`;
    const momentCount = visibleThreads.length;
    const weight = Math.max(1, momentCount);
    const collapsedLabel =
      momentCount > 0 ? `${bucket.label} · ${momentCount}` : bucket.label;

    sessionNodeIds.push(sessionId);
    sessionTimestamps.set(sessionId, parseTimestamp(bucket.threads[0]?.timestamp ?? ""));
    const labelMax = density === "rail" ? 22 : 34;
    nodes.push({
      id: sessionId,
      kind: "session",
      label: truncateLabel(showMomentsFull ? bucket.label : collapsedLabel, labelMax),
      sessionId: bucket.sessionId,
      x: 0,
      y: 0,
      radius: sessionRadius(momentCount, density),
      weight,
      hue: sessionHue(bucket.sessionId),
      visible: true,
      expanded: showMomentsFull,
      momentCount,
      showLabel: momentCount >= 3 || weight >= 4,
      renderMode: "full",
    });

    const threadsToShow =
      needle && !sessionMatches ? matchingThreads : visibleThreads;
    const ghostLimit = showMomentsFull ? threadsToShow.length : MAX_GHOST_MOMENTS;

    threadsToShow.slice(0, ghostLimit).forEach((thread, index) => {
      const isGhost = !showMomentsFull;
      const threadId = `thread:${thread.sync_key}`;
      const baseRadius = threadRadius(thread, density);
      nodes.push({
        id: threadId,
        kind: "thread",
        label: truncateLabel(humanMomentTitle(thread), density === "rail" ? 18 : 30),
        sessionId: bucket.sessionId,
        syncKey: thread.sync_key,
        x: 0,
        y: 0,
        radius: isGhost ? baseRadius * 0.62 : baseRadius,
        weight: threadWeight(thread),
        hue: sessionHue(bucket.sessionId),
        visible: true,
        showLabel: !isGhost && index < (density === "rail" ? 2 : 4),
        renderMode: isGhost ? "ghost" : "full",
      });

      edges.push({
        id: `membership:${sessionId}:${threadId}`,
        from: sessionId,
        to: threadId,
        kind: "membership",
        visible: true,
        strength: isGhost ? 0.16 : 0.11,
        renderMode: isGhost ? "ghost" : "full",
      });

      if (index > 0 && !isGhost) {
        const prevId = `thread:${threadsToShow[index - 1].sync_key}`;
        edges.push({
          id: `sequence:${prevId}:${threadId}`,
          from: prevId,
          to: threadId,
          kind: "sequence",
          visible: true,
          strength: 0.06,
          renderMode: "full",
        });
      }
    });
  }

  edges.push(...buildSessionProximityEdges(sessionNodeIds, sessionTimestamps));

  const visibleSessionIds = nodes
    .filter((node) => node.kind === "session")
    .map((node) => node.sessionId);
  const sessionIdSet = new Set(visibleSessionIds);

  if (vaultNotes.length > 0) {
    const noteSlice = buildNoteGraphSlice(vaultNotes, visibleSessionIds, {
      searchQuery: needle,
      labelMax: density === "rail" ? 18 : 28,
      sessionNodeExists: (sessionId) => sessionIdSet.has(sessionId),
    });
    nodes.push(...noteSlice.nodes);
    edges.push(...noteSlice.edges);
  }

  seedNodePositions(nodes, layoutWidth, layoutHeight, priorPositions);

  return {
    nodes,
    edges,
    sessionCount: nodes.filter((node) => node.kind === "session").length,
    momentCount: nodes.filter((node) => node.kind === "thread").length,
    width: layoutWidth,
    height: layoutHeight,
  };
}

export function findMapNode(
  graph: ContextMapGraph,
  nodeId: string | null,
): ContextMapNode | null {
  if (!nodeId) return null;
  return graph.nodes.find((node) => node.id === nodeId) ?? null;
}
