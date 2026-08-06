/** Lightweight fuzzy scoring for Code Quick Open and path pickers. */

export function fuzzyScorePath(query: string, text: string): number {
  if (!query) return 1;
  const needle = query.toLowerCase();
  const haystack = text.toLowerCase();
  if (haystack.startsWith(needle)) return 240 + needle.length;
  if (haystack.includes(needle)) return 140 + needle.length;

  let queryIndex = 0;
  let streak = 0;
  let score = 0;
  for (let i = 0; i < haystack.length && queryIndex < needle.length; i += 1) {
    if (haystack[i] === needle[queryIndex]) {
      queryIndex += 1;
      streak += 1;
      score += 10 + streak;
      // Prefer matches after path separators / camel boundaries.
      if (
        i === 0 ||
        haystack[i - 1] === "/" ||
        haystack[i - 1] === "-" ||
        haystack[i - 1] === "_" ||
        (haystack[i] >= "a" &&
          haystack[i] <= "z" &&
          text[i] === text[i].toUpperCase() &&
          text[i] !== text[i].toLowerCase())
      ) {
        score += 18;
      }
    } else {
      streak = 0;
    }
  }
  return queryIndex === needle.length ? score : 0;
}

export function fuzzyMatchPaths<T extends { path: string }>(
  files: T[],
  query: string,
  limit = 80,
): T[] {
  const trimmed = query.trim().toLowerCase().replace(/^>/, "");
  if (!trimmed) return files.slice(0, limit);

  return files
    .map((file, index) => {
      const path = file.path.toLowerCase().replaceAll("\\", "/");
      const name = path.split("/").pop() ?? path;
      const score = Math.max(
        fuzzyScorePath(trimmed, name) * 1.15,
        fuzzyScorePath(trimmed, path),
        fuzzyScorePath(trimmed.replaceAll(" ", ""), name.replaceAll(/[-_]/g, "")),
      );
      return { file, score, index };
    })
    .filter((row) => row.score > 0)
    .sort(
      (left, right) =>
        right.score - left.score ||
        left.file.path.localeCompare(right.file.path) ||
        left.index - right.index,
    )
    .slice(0, limit)
    .map((row) => row.file);
}
