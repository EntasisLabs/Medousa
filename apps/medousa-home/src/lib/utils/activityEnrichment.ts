import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard, WorkspaceEvent } from "$lib/types/workspace";
import { formatCardTitle } from "$lib/utils/formatWork";
import { formatToolName } from "$lib/utils/formatTurn";

export interface ActivityEnrichment {
  card?: WorkCard;
  detail?: WorkCardDetail;
}

export function cardRefId(event: WorkspaceEvent): string | null {
  const reference = event.refs.find((ref) => ref.ref_type === "card");
  return reference?.ref_id ?? null;
}

export function vaultRefPath(event: WorkspaceEvent): string | null {
  const reference = event.refs.find((ref) => ref.ref_type === "vault_path");
  return reference?.ref_id ?? null;
}

export function collectActivityCardIds(events: WorkspaceEvent[]): string[] {
  const ids = new Set<string>();
  for (const event of events) {
    const cardId = cardRefId(event);
    if (cardId) ids.add(cardId);
  }
  return [...ids];
}

export function resolveActivityEnrichment(
  event: WorkspaceEvent,
  cardsById: ReadonlyMap<string, WorkCard>,
  detailsById: ReadonlyMap<string, WorkCardDetail>,
): ActivityEnrichment {
  const cardId = cardRefId(event);
  if (!cardId) return {};
  return {
    card: cardsById.get(cardId),
    detail: detailsById.get(cardId),
  };
}

function isSlugLikeTitle(title: string): boolean {
  const trimmed = title.trim();
  return (
    trimmed.length > 0 &&
    trimmed.length <= 28 &&
    !trimmed.includes(" ") &&
    /^[a-z0-9._-]+$/i.test(trimmed)
  );
}

function looksLikePath(value: string): boolean {
  return value.includes("/") || /\.md$/i.test(value);
}

function looksLikeOpaquePathStem(stem: string): boolean {
  const trimmed = stem.trim();
  if (!trimmed) return true;
  if (/^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(trimmed)) {
    return true;
  }
  if (/^daily-[0-9a-f]{8,}/i.test(trimmed)) return true;
  if (trimmed.length >= 20 && /^[0-9a-f-]+$/i.test(trimmed)) return true;
  return false;
}

/** Soft path whisper for activity — never dump UUID filenames. */
export function calmVaultPathHint(path: string, summaryOrTitle = ""): string {
  const parts = path.replaceAll("\\", "/").split("/").filter(Boolean);
  const file = parts.pop() ?? path;
  const stem = file.replace(/\.md$/i, "");
  const folder = parts[parts.length - 1] ?? "";
  const title = summaryOrTitle.trim().toLowerCase();

  if (looksLikeOpaquePathStem(stem)) {
    if (!folder || /^(journal|daily|notes|vault)$/i.test(folder)) return "";
    if (title.includes(folder.toLowerCase())) return "";
    return folder;
  }

  const human = stem.replaceAll("-", " ").replaceAll("_", " ").trim();
  if (human && title.includes(human.toLowerCase())) {
    if (
      folder &&
      !title.includes(folder.toLowerCase()) &&
      !/^(journal|daily|notes|vault)$/i.test(folder)
    ) {
      return folder;
    }
    return "";
  }

  if (folder && !/^(journal|daily|notes|vault)$/i.test(folder)) {
    return `${folder} · ${human}`;
  }
  return human;
}

export function formatWorkerIntent(raw?: string | null): string {
  switch (raw?.trim().toLowerCase()) {
    case "research":
    case "delegate.research":
    case "web":
    case "websearch":
      return "Research";
    case "memory.context":
    case "memory_context":
      return "Memory";
    case "memory.avec_calibrate":
    case "avec_calibrate":
      return "Memory calibration";
    case "general":
    case "default":
      return "General task";
    default:
      return raw?.replaceAll("_", " ").trim() ?? "";
  }
}

export function formatJobFamily(jobType?: string | null): string {
  if (!jobType?.trim()) return "";
  if (jobType === "daemon.ask") return "Background ask";
  const tail = jobType.split(".").pop() ?? jobType;
  return tail.replaceAll("_", " ");
}

function humanizeWrappingReason(reason: string): string {
  switch (reason) {
    case "synthesis_pending":
      return "synthesis pending";
    case "delivery_pending":
      return "delivery pending";
    default:
      return reason.replaceAll("_", " ");
  }
}

export function formatActivityTools(
  toolNames?: string[] | null,
  max = 3,
): string {
  if (!toolNames?.length) return "";
  const formatted = toolNames.slice(0, max).map(formatToolName);
  const extra = toolNames.length > max ? ` +${toolNames.length - max}` : "";
  return `${formatted.join(", ")}${extra}`;
}

export function resolveTaskTitle(
  enrichment: ActivityEnrichment,
  detailLine?: string | null,
): string {
  if (detailLine?.trim()) return detailLine.trim();

  const { card, detail } = enrichment;
  if (detail?.task_line?.trim()) {
    const task = detail.task_line.trim();
    const title = card ? formatCardTitle(card).trim() : "";
    if (!title || isSlugLikeTitle(title)) return task;
  }

  if (card) {
    const title = formatCardTitle(card).trim();
    if (title && !isSlugLikeTitle(title)) return title;
  }

  if (detail?.kind === "turn_worker") {
    const intent = formatWorkerIntent(detail.subtitle);
    if (intent) return intent;
  }

  if (detail?.kind === "ask_job" || detail?.job_type === "daemon.ask") {
    return "background ask";
  }

  if (detail?.job_type) {
    const family = formatJobFamily(detail.job_type);
    if (family) return family;
  }

  if (card) {
    const title = formatCardTitle(card).trim();
    if (title) return title;
  }

  return "";
}

export function buildActivityContext(
  event: WorkspaceEvent,
  enrichment: ActivityEnrichment,
): string {
  const titleHint = event.detail_line?.trim() ?? "";
  const vaultPath = vaultRefPath(event);
  const isVaultKind =
    event.kind === "vault_note_updated" || event.kind === "vault_note_created";

  if (isVaultKind || vaultPath) {
    const path =
      vaultPath ??
      (event.context_line?.trim() && looksLikePath(event.context_line)
        ? event.context_line.trim()
        : null);
    return path ? calmVaultPathHint(path, titleHint) : "";
  }

  if (event.context_line?.trim()) {
    const line = event.context_line.trim();
    if (looksLikePath(line)) return calmVaultPathHint(line, titleHint);
    return line;
  }

  const { detail } = enrichment;
  const parts: string[] = [];
  const taskTitle = resolveTaskTitle(
    enrichment,
    event.detail_line,
  ).toLowerCase();

  if (detail?.kind === "turn_worker") {
    const intent = formatWorkerIntent(detail.subtitle);
    if (intent && intent.toLowerCase() !== taskTitle) {
      parts.push(intent);
    }
    const tools = formatActivityTools(detail.tool_names);
    if (tools) parts.push(tools);
  } else if (detail?.kind === "ask_job" || detail?.job_type === "daemon.ask") {
    parts.push("Background ask");
  } else if (detail?.job_type) {
    const family = formatJobFamily(detail.job_type);
    if (family) parts.push(family);
  }

  if (event.kind === "work_wrapping_up" && detail?.wrapping_up_reasons?.length) {
    parts.push(
      detail.wrapping_up_reasons.map(humanizeWrappingReason).join(", "),
    );
  }

  return parts.join(" · ");
}
