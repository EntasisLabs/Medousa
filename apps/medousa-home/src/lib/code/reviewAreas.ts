import type { ReviewFileChange } from "$lib/forge";

/** null is all files; the empty directory is the repository root only. */
export function reviewFileInDirectory(path: string, directory: string | null): boolean {
  return directory === null || (directory === "" ? !path.includes("/") : path.startsWith(`${directory}/`));
}

export function reviewAreas(files: ReviewFileChange[]) {
  const groups = new Map<string, number>();
  for (const file of files) {
    const directory = file.path.includes("/") ? file.path.split("/")[0] : "";
    groups.set(directory, (groups.get(directory) ?? 0) + 1);
  }
  return [...groups].map(([directory, count]) => ({ directory, label: directory ? `${directory}/` : "Repository root", count })).sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
}
