import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";

export function setNoteTitleInContent(content: string, title: string): string {
  const trimmedTitle = title.trim();
  if (!trimmedTitle) return content;

  const { content: body, frontmatter } = stripFrontmatter(content);
  const lines = body.split("\n");
  const headingIndex = lines.findIndex((line) => /^#\s+/.test(line));

  if (headingIndex >= 0) {
    lines[headingIndex] = `# ${trimmedTitle}`;
  } else {
    lines.unshift(`# ${trimmedTitle}`, "");
  }

  const nextBody = lines.join("\n").replace(/^\n+/, "");
  if (!frontmatter) return nextBody;
  return `---\n${frontmatter}\n---\n\n${nextBody}`;
}

export function normalizeVaultNotePath(path: string): string {
  const trimmed = path.trim().replace(/^\/+/, "").replace(/\\/g, "/");
  if (!trimmed) return "";
  return trimmed.endsWith(".md") ? trimmed : `${trimmed}.md`;
}
