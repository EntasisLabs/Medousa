const DEFAULT_CHUNK_CHARS = 280;

/** Turn rendered Markdown into calm speech instead of reading its punctuation aloud. */
export function narrationTextFromMarkdown(markdown: string): string {
  return markdown
    .replace(/```[\s\S]*?```/g, " Code block available in the written response. ")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/<https?:\/\/[^>]+>/gi, " link ")
    .replace(/<[^>]+>/g, " ")
    .replace(/https?:\/\/\S+/gi, " link ")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/gm, "")
    .replace(/^\s*>\s?/gm, "")
    .replace(/^\s*[-*+]\s+/gm, "")
    .replace(/^\s*\d+[.)]\s+/gm, "")
    .replace(/[|*_~]/g, " ")
    .replace(/&amp;/gi, "and")
    .replace(/&lt;/gi, "less than")
    .replace(/&gt;/gi, "greater than")
    .replace(/&quot;/gi, '"')
    .replace(/&#39;/gi, "'")
    .replace(/\s+([,.;:!?])/g, "$1")
    .replace(/\s+/g, " ")
    .trim();
}

/** Keep native speech engines responsive by feeding them sentence-sized utterances. */
export function narrationChunks(
  text: string,
  maxChars = DEFAULT_CHUNK_CHARS,
): string[] {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (!normalized) return [];
  const limit = Math.max(80, maxChars);
  if (normalized.length <= limit) return [normalized];

  const sentences = normalized.match(/[^.!?]+[.!?]+|[^.!?]+$/g) ?? [normalized];
  const chunks: string[] = [];
  let current = "";
  for (const rawSentence of sentences) {
    const sentence = rawSentence.trim();
    if (!sentence) continue;
    for (const piece of splitLongSpeechPiece(sentence, limit)) {
      const candidate = current ? `${current} ${piece}` : piece;
      if (candidate.length <= limit) {
        current = candidate;
      } else {
        if (current) chunks.push(current);
        current = piece;
      }
    }
  }
  if (current) chunks.push(current);
  return chunks;
}

function splitLongSpeechPiece(value: string, limit: number): string[] {
  if (value.length <= limit) return [value];
  const words = value.split(/\s+/);
  const pieces: string[] = [];
  let current = "";
  for (const word of words) {
    const candidate = current ? `${current} ${word}` : word;
    if (candidate.length <= limit) {
      current = candidate;
      continue;
    }
    if (current) pieces.push(current);
    if (word.length <= limit) {
      current = word;
      continue;
    }
    for (let offset = 0; offset < word.length; offset += limit) {
      pieces.push(word.slice(offset, offset + limit));
    }
    current = "";
  }
  if (current) pieces.push(current);
  return pieces;
}
