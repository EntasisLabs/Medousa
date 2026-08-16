export const VAULT_LIST_PAGE_LIMIT = 200;
export const VAULT_LIST_MAX_PAGES = 50;

/** True when paging stopped before the daemon said the listing was complete. */
export function listingIncompleteAfterPages(
  pagesFetched: number,
  truncated: boolean,
  nextCursor: string | null | undefined,
): boolean {
  return (
    pagesFetched >= VAULT_LIST_MAX_PAGES &&
    Boolean(truncated && nextCursor)
  );
}
