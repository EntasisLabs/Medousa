import type { ContextMapEdgeKind, ContextMapNodeKind } from "$lib/utils/contextMap";

export interface SimNodeInput {
  id: string;
  kind: ContextMapNodeKind;
  radius: number;
  weight: number;
  /** Optional seed; preserved across topology updates when omitted for existing ids. */
  x?: number;
  y?: number;
}

export interface SimEdgeInput {
  id: string;
  from: string;
  to: string;
  kind: ContextMapEdgeKind;
  strength?: number;
  ghost?: boolean;
}

export interface SimPosition {
  x: number;
  y: number;
}

interface SimBody {
  id: string;
  kind: ContextMapNodeKind;
  radius: number;
  weight: number;
  mass: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  fx: number | null;
  fy: number | null;
}

interface SimLink {
  from: string;
  to: string;
  kind: ContextMapEdgeKind;
  strength: number;
  distance: number;
}

export interface ContextMapSimulation {
  getPositions(): Map<string, SimPosition>;
  setTopology(
    nodes: SimNodeInput[],
    edges: SimEdgeInput[],
    width: number,
    height: number,
  ): void;
  /** Advance one step. Returns true while still awake. */
  tick(): boolean;
  restart(options?: { alpha?: number }): void;
  pin(id: string, x: number, y: number): void;
  unpin(id: string): void;
  isSleeping(): boolean;
  dispose(): void;
}

const ALPHA_MIN = 0.02;
const ALPHA_DECAY = 0.032;
const VELOCITY_DECAY = 0.84;
/** Near-zero centering — only a hint so the graph doesn't drift forever. */
const CENTER_STRENGTH = 0.0006;
const CHARGE_BASE = 560;
const GRID_CELL = 72;
/** Keep sim awake while dragging without running a hard re-collapse. */
const DRAG_ALPHA_FLOOR = 0.18;

function nodeMass(kind: ContextMapNodeKind, weight: number): number {
  if (kind === "session") return 2.4 + weight * 0.12;
  return 1 + weight * 0.08;
}

function linkDistance(
  kind: ContextMapEdgeKind,
  fromR: number,
  toR: number,
  ghost: boolean,
): number {
  if (kind === "session_chain") return 220 + (fromR + toR) * 1.5;
  if (kind === "note_tag") return 160 + (fromR + toR) * 1.2;
  if (kind === "note_link") return 90 + (fromR + toR) * 0.8;
  if (kind === "note_session") return 96 + fromR + toR * 0.5;
  if (kind === "membership") {
    const base = 72 + fromR + toR * 0.35;
    return ghost ? base * 0.78 : base;
  }
  return 54 + (fromR + toR) * 0.45;
}

function linkStrength(kind: ContextMapEdgeKind, strength?: number): number {
  if (strength != null) {
    const scale =
      kind === "session_chain" || kind === "note_tag"
        ? 1.1
        : kind === "note_link" || kind === "note_session"
          ? 2.8
          : 2.2;
    return Math.min(0.55, Math.max(0.01, strength * scale));
  }
  if (kind === "membership") return 0.38;
  if (kind === "sequence") return 0.16;
  if (kind === "note_session") return 0.32;
  if (kind === "note_link") return 0.4;
  if (kind === "note_tag") return 0.02;
  // Proximity hints — not hard tethers.
  return 0.018;
}

function cellKey(cx: number, cy: number): string {
  return `${cx},${cy}`;
}

export function createContextMapSimulation(): ContextMapSimulation {
  const bodies = new Map<string, SimBody>();
  let links: SimLink[] = [];
  let width = 800;
  let height = 600;
  let alpha = 0;
  let disposed = false;
  let pinnedCount = 0;

  function seedBody(input: SimNodeInput, index: number, total: number): SimBody {
    const cx = width / 2;
    const cy = height / 2;
    const angle = (Math.PI * 2 * index) / Math.max(total, 1) - Math.PI / 2;
    const spread = Math.min(width, height) * 0.36;
    const x =
      input.x ??
      cx + Math.cos(angle) * spread * (0.72 + (input.weight / 12) * 0.3);
    const y =
      input.y ??
      cy + Math.sin(angle) * spread * (0.72 + (input.weight / 12) * 0.3);
    return {
      id: input.id,
      kind: input.kind,
      radius: input.radius,
      weight: input.weight,
      mass: nodeMass(input.kind, input.weight),
      x,
      y,
      vx: 0,
      vy: 0,
      fx: null,
      fy: null,
    };
  }

  function setTopology(
    nodes: SimNodeInput[],
    edges: SimEdgeInput[],
    nextWidth: number,
    nextHeight: number,
  ): void {
    if (disposed) return;
    width = Math.max(120, nextWidth);
    height = Math.max(120, nextHeight);

    const keep = new Set(nodes.map((node) => node.id));
    for (const id of [...bodies.keys()]) {
      if (!keep.has(id)) bodies.delete(id);
    }

    nodes.forEach((input, index) => {
      const existing = bodies.get(input.id);
      if (existing) {
        existing.kind = input.kind;
        existing.radius = input.radius;
        existing.weight = input.weight;
        existing.mass = nodeMass(input.kind, input.weight);
        if (input.x != null && input.y != null && existing.fx == null) {
          // Optional explicit reseeding only when caller provides coords for new layout space.
        }
        return;
      }
      const seeded = seedBody(
        {
          ...input,
          x: input.x,
          y: input.y,
        },
        index,
        nodes.length,
      );
      // Place new moments near their session if present in seed coords.
      if (input.x != null && input.y != null) {
        seeded.x = input.x;
        seeded.y = input.y;
      }
      bodies.set(input.id, seeded);
    });

    links = edges
      .filter((edge) => bodies.has(edge.from) && bodies.has(edge.to))
      .map((edge) => {
        const from = bodies.get(edge.from)!;
        const to = bodies.get(edge.to)!;
        return {
          from: edge.from,
          to: edge.to,
          kind: edge.kind,
          strength: linkStrength(edge.kind, edge.strength),
          distance: linkDistance(edge.kind, from.radius, to.radius, Boolean(edge.ghost)),
        };
      });

    restart({ alpha: Math.max(alpha, 0.75) });
  }

  function restart(options?: { alpha?: number }): void {
    if (disposed) return;
    alpha = Math.max(alpha, options?.alpha ?? 0.85);
  }

  function pin(id: string, x: number, y: number): void {
    const body = bodies.get(id);
    if (!body) return;
    const wasPinned = body.fx != null;
    body.fx = x;
    body.fy = y;
    body.x = x;
    body.y = y;
    body.vx = 0;
    body.vy = 0;
    if (!wasPinned) pinnedCount += 1;
    alpha = Math.max(alpha, DRAG_ALPHA_FLOOR);
  }

  function unpin(id: string): void {
    const body = bodies.get(id);
    if (!body || body.fx == null) return;
    body.fx = null;
    body.fy = null;
    pinnedCount = Math.max(0, pinnedCount - 1);
  }

  function applyRepulsion(list: SimBody[]): void {
    const grid = new Map<string, SimBody[]>();
    for (const body of list) {
      const cx = Math.floor(body.x / GRID_CELL);
      const cy = Math.floor(body.y / GRID_CELL);
      const key = cellKey(cx, cy);
      const bucket = grid.get(key);
      if (bucket) bucket.push(body);
      else grid.set(key, [body]);
    }

    const chargeScale = CHARGE_BASE * alpha;
    for (const body of list) {
      const cx = Math.floor(body.x / GRID_CELL);
      const cy = Math.floor(body.y / GRID_CELL);
      for (let ox = -1; ox <= 1; ox += 1) {
        for (let oy = -1; oy <= 1; oy += 1) {
          const bucket = grid.get(cellKey(cx + ox, cy + oy));
          if (!bucket) continue;
          for (const other of bucket) {
            if (other.id <= body.id) continue;
            let dx = other.x - body.x;
            let dy = other.y - body.y;
            let dist2 = dx * dx + dy * dy;
            if (dist2 < 1) {
              dx = 0.5;
              dy = 0;
              dist2 = 0.25;
            }
            const dist = Math.sqrt(dist2);
            const minDist = body.radius + other.radius + 8;
            const soft = Math.max(dist, minDist * 0.55);
            const force =
              ((chargeScale * (body.mass + other.mass)) / 2) / (soft * soft);
            const fx = (dx / dist) * force;
            const fy = (dy / dist) * force;
            body.vx -= fx / body.mass;
            body.vy -= fy / body.mass;
            other.vx += fx / other.mass;
            other.vy += fy / other.mass;
          }
        }
      }
    }
  }

  function applyLinks(dragging: boolean): void {
    for (const link of links) {
      const from = bodies.get(link.from);
      const to = bodies.get(link.to);
      if (!from || !to) continue;
      // While dragging, drop loose tethers so the graph doesn't re-bunch.
      // Keep membership / note_session / note_link so satellites still follow.
      let strengthScale = 1;
      if (dragging && (link.kind === "session_chain" || link.kind === "note_tag")) {
        strengthScale = 0;
      } else if (dragging && link.kind === "sequence") {
        strengthScale = 0.35;
      }
      if (strengthScale <= 0) continue;

      let dx = to.x - from.x;
      let dy = to.y - from.y;
      let dist = Math.hypot(dx, dy);
      if (dist < 1e-4) {
        dx = 0.01;
        dy = 0;
        dist = 0.01;
      }
      const bias =
        ((dist - link.distance) / dist) * link.strength * strengthScale * alpha;
      const fx = dx * bias;
      const fy = dy * bias;
      const fromMass = from.mass;
      const toMass = to.mass;
      const sum = fromMass + toMass;
      from.vx += (fx * toMass) / sum;
      from.vy += (fy * toMass) / sum;
      to.vx -= (fx * fromMass) / sum;
      to.vy -= (fy * fromMass) / sum;
    }
  }

  function applyCenter(list: SimBody[]): void {
    // No centering while dragging — that constant pull collapses custom layouts.
    if (pinnedCount > 0) return;
    const cx = width / 2;
    const cy = height / 2;
    const k = CENTER_STRENGTH * alpha;
    for (const body of list) {
      body.vx += (cx - body.x) * k;
      body.vy += (cy - body.y) * k;
    }
  }

  function integrate(list: SimBody[]): void {
    for (const body of list) {
      if (body.fx != null && body.fy != null) {
        body.x = body.fx;
        body.y = body.fy;
        body.vx = 0;
        body.vy = 0;
        continue;
      }
      body.vx *= VELOCITY_DECAY;
      body.vy *= VELOCITY_DECAY;
      body.x += body.vx;
      body.y += body.vy;
    }
  }

  function tick(): boolean {
    if (disposed) return false;
    if (alpha < ALPHA_MIN && pinnedCount === 0) {
      alpha = 0;
      return false;
    }

    const list = [...bodies.values()];
    if (list.length === 0) {
      alpha = 0;
      return false;
    }

    const dragging = pinnedCount > 0;
    if (dragging) {
      alpha = Math.max(alpha, DRAG_ALPHA_FLOOR);
    }

    applyRepulsion(list);
    applyLinks(dragging);
    applyCenter(list);
    integrate(list);

    alpha *= 1 - ALPHA_DECAY;
    if (alpha < ALPHA_MIN && pinnedCount === 0) {
      alpha = 0;
      return false;
    }
    return true;
  }

  function getPositions(): Map<string, SimPosition> {
    const out = new Map<string, SimPosition>();
    for (const body of bodies.values()) {
      out.set(body.id, { x: body.x, y: body.y });
    }
    return out;
  }

  function isSleeping(): boolean {
    return alpha < ALPHA_MIN && pinnedCount === 0;
  }

  function dispose(): void {
    disposed = true;
    bodies.clear();
    links = [];
    alpha = 0;
    pinnedCount = 0;
  }

  return {
    getPositions,
    setTopology,
    tick,
    restart,
    pin,
    unpin,
    isSleeping,
    dispose,
  };
}

/** Seed new nodes near an existing parent (e.g. session) with a small jitter. */
export function seedNear(
  parent: SimPosition | undefined,
  index: number,
  fallback: SimPosition,
): SimPosition {
  if (!parent) return fallback;
  const angle = index * 2.4;
  const radius = 28 + (index % 5) * 6;
  return {
    x: parent.x + Math.cos(angle) * radius,
    y: parent.y + Math.sin(angle) * radius,
  };
}
