/** Significant text for “did we eat content?” checks (fences + prose). */

import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";

export function significantLiveText(markdown: string): string {
  const { content } = stripFrontmatter(markdown);
  return content
    .replace(/```[\s\S]*?```/g, (block) => block)
    .replace(/\s+/g, " ")
    .trim();
}
