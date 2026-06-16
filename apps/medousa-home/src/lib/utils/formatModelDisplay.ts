/** Short label for composer model pill (Cursor-style). Prefer resolveModelDisplayLabel when provider is known. */
export function formatModelDisplayName(model: string, maxLen = 22): string {
  const trimmed = model.trim();
  if (!trimmed) return "Model";
  if (trimmed.length <= maxLen) return trimmed;
  return `${trimmed.slice(0, maxLen - 1)}…`;
}

export function modelPickKey(provider: string, model: string): string {
  return `${provider.trim().toLowerCase()}:${model.trim()}`;
}
