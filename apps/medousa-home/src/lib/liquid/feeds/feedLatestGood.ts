/**
 * Latest-good feed result helpers for Liquid ```feed fences (Phase D).
 */

import { fetchFeedLatestGood } from "$lib/daemon";

export type FeedDatatype = "md" | "text" | "json" | "csv" | "image";

export interface FeedLatestGoodResult {
  feedId: string;
  datatype: FeedDatatype;
  body: string;
  jobId?: string | null;
  finishedAt?: string | null;
}

const DATATYPES = new Set<FeedDatatype>(["md", "text", "json", "csv", "image"]);

function normalizeDatatype(raw: string | undefined, fallback: FeedDatatype): FeedDatatype {
  const value = (raw ?? "").trim().toLowerCase();
  if (DATATYPES.has(value as FeedDatatype)) return value as FeedDatatype;
  return fallback;
}

export async function readFeedLatestGood(
  feedId: string,
  expectedDatatype?: FeedDatatype,
  profileId?: string,
): Promise<FeedLatestGoodResult | null> {
  const id = feedId.trim();
  if (!id) return null;
  try {
    const res = await fetchFeedLatestGood(id, profileId);
    if (!res?.body?.trim()) return null;
    return {
      feedId: res.feedId,
      datatype: normalizeDatatype(res.datatype, expectedDatatype ?? "text"),
      body: res.body,
      jobId: res.jobId ?? null,
      finishedAt: res.finishedAt ?? null,
    };
  } catch {
    return null;
  }
}

export function csvToMarkdownTable(csv: string): string {
  const lines = csv
    .replace(/\r\n/g, "\n")
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  if (lines.length === 0) return "";
  const rows = lines.map((line) => line.split(",").map((cell) => cell.trim()));
  const headers = rows[0] ?? [];
  if (headers.length === 0) return "";
  const divider = `| ${headers.map(() => "---").join(" | ")} |`;
  const headerRow = `| ${headers.join(" | ")} |`;
  const body = rows
    .slice(1)
    .map((row) => `| ${row.join(" | ")} |`)
    .join("\n");
  return body ? `${headerRow}\n${divider}\n${body}` : `${headerRow}\n${divider}`;
}

export function prettyJsonBody(body: string): string {
  try {
    return JSON.stringify(JSON.parse(body), null, 2);
  } catch {
    return body;
  }
}
