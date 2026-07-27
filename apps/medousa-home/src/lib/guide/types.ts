export type GuideGroupId = "start" | "everyday" | "create" | "connect" | "more";

export interface GuideChapter {
  id: string;
  title: string;
  /** Filename under pages/ without path, e.g. `00-welcome.md`. */
  file: string;
  group: GuideGroupId;
  summary: string;
}

export interface GuideGroup {
  id: GuideGroupId;
  label: string;
}

export interface GuideHandoff {
  chapterId: string;
  anchor?: string | null;
}
