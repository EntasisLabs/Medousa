import type { LocusNodeSummary } from "$lib/types/locus";
import { humanMomentTitle, sessionMapLabel } from "$lib/utils/contextHuman";

export type ContextMapNodeKind = "session" | "thread" | "claim" | "note";

export type ContextMapEdgeKind = "membership" | "sequence" | "session_chain";

export type ContextMapRenderMode = "full" | "ghost";

export interface ContextMapNode {
  id: string;
  kind: ContextMapNodeKind;
  label: string;
  sessionId: string;
  syncKey?: string;
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

function layoutForceDirected(
  nodes: ContextMapNode[],
  edges: ContextMapEdge[],
  width: number,
  height: number,
): void {
  const visibleNodes = nodes.filter((node) => node.visible);
  if (visibleNodes.length === 0) return;

  const cx = width / 2;
  const cy = height / 2;

  visibleNodes.forEach((node, index) => {
    if (node.x !== 0 || node.y !== 0) return;
    const angle = (Math.PI * 2 * index) / visibleNodes.length - Math.PI / 2;
    const spread = Math.min(width, height) * 0.38;
    node.x = cx + Math.cos(angle) * spread * (0.75 + (node.weight / 12) * 0.35);
    node.y = cy + Math.sin(angle) * spread * (0.75 + (node.weight / 12) * 0.35);
  });

  const nodeById = Object.fromEntries(visibleNodes.map((node) => [node.id, node]));
  const visibleEdges = edges.filter(
    (edge) => edge.visible && nodeById[edge.from] && nodeById[edge.to],
  );

  const iterations = Math.min(240, 100 + visibleNodes.length * 4);

  for (let step = 0; step < iterations; step += 1) {
    const cooling = 1 - step / iterations;

    for (let i = 0; i < visibleNodes.length; i += 1) {
      for (let j = i + 1; j < visibleNodes.length; j += 1) {
        const a = visibleNodes[i];
        const b = visibleNodes[j];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let dist = Math.hypot(dx, dy);
        if (dist < 1) {
          dist = 1;
          dx = 1;
          dy = 0;
        }
        const mass = (node: ContextMapNode) =>
          node.kind === "session" ? 5200 + node.weight * 420 : 2400 + node.weight * 180;
        const repulse = ((mass(a) + mass(b)) / 2 / (dist * dist)) * cooling;
        const fx = (dx / dist) * repulse;
        const fy = (dy / dist) * repulse;
        a.x -= fx;
        a.y -= fy;
        b.x += fx;
        b.y += fy;
      }
    }

    for (const edge of visibleEdges) {
      const from = nodeById[edge.from];
      const to = nodeById[edge.to];
      if (!from || !to) continue;
      const dx = to.x - from.x;
      const dy = to.y - from.y;
      const dist = Math.hypot(dx, dy) || 1;
      const strength = edge.strength ?? (edge.kind === "session_chain" ? 0.06 : 0.1);
      const ghostPull = edge.renderMode === "ghost" ? 1.35 : 1;
      const target =
        edge.kind === "session_chain"
          ? 140 + (from.radius + to.radius) * 1.4
          : edge.kind === "membership"
            ? (72 + from.radius + to.radius * 0.35) / ghostPull
            : 52 + (from.radius + to.radius) * 0.45;
      const offset = (dist - target) * strength * cooling;
      const fx = (dx / dist) * offset;
      const fy = (dy / dist) * offset;
      from.x += fx;
      from.y += fy;
      to.x -= fx;
      to.y -= fy;
    }

    for (const node of visibleNodes) {
      node.x += (cx - node.x) * 0.008 * cooling;
      node.y += (cy - node.y) * 0.008 * cooling;
    }
  }
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

  if (node.kind === "session") {
    const total = node.momentCount ?? moments;
    return `${total} moment${total === 1 ? "" : "s"} in this session`;
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

export function applyPinnedPositions(
  graph: ContextMapGraph,
  pins: Map<string, { x: number; y: number }>,
): ContextMapGraph {
  if (pins.size === 0) return graph;
  return {
    ...graph,
    nodes: graph.nodes.map((node) => {
      const pin = pins.get(node.id);
      return pin ? { ...node, x: pin.x, y: pin.y } : node;
    }),
  };
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
  },
): ContextMapGraph {
  const {
    width,
    height,
    expandedSessionIds,
    searchQuery = "",
    density = "default",
  } = options;
  const needle = searchQuery.trim().toLowerCase();
  const buckets = buildSessionBuckets(locusNodes, sessionLabels);

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
    const visibleThreads = bucket.threads.slice(0, MAX_THREADS_PER_SESSION);
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

    const showSession = !needle || sessionMatches || matchingThreads.length > 0;
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

  layoutForceDirected(nodes, edges, layoutWidth, layoutHeight);

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
