/** Preserve the live viewport when older transcript content is prepended. */
export function scrollTopAfterHistoryPrepend(
  currentTop: number,
  previousAnchorOffset: number,
  nextAnchorOffset: number,
): number {
  const addedHeight = Math.max(0, nextAnchorOffset - previousAnchorOffset);
  return Math.max(0, currentTop + addedHeight);
}
