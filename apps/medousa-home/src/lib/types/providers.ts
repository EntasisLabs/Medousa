export interface ProviderCategory {
  id: string;
  label: string;
}

export interface ProviderCatalogEntry {
  id: string;
  label: string;
  category: string;
  defaultModel: string;
  needsApiKey: boolean;
  supportsCustomBaseUrl: boolean;
  defaultBaseUrl: string | null;
  keyHint: string | null;
  blurb: string;
}

export interface ProvidersListResult {
  categories: ProviderCategory[];
  providers: ProviderCatalogEntry[];
}

export const PROVIDER_CATEGORY_ORDER = ["featured", "local", "cloud"] as const;

export function categoryLabel(
  categories: ProviderCategory[],
  id: string,
): string {
  return categories.find((entry) => entry.id === id)?.label ?? id;
}

export function filterProviders(
  providers: ProviderCatalogEntry[],
  query: string,
  excludeIds: string[] = [],
): ProviderCatalogEntry[] {
  const excluded = new Set(excludeIds.map((id) => id.toLowerCase()));
  const needle = query.trim().toLowerCase();
  return providers.filter((entry) => {
    if (excluded.has(entry.id.toLowerCase())) return false;
    if (!needle) return true;
    return (
      entry.id.toLowerCase().includes(needle) ||
      entry.label.toLowerCase().includes(needle) ||
      entry.blurb.toLowerCase().includes(needle)
    );
  });
}

export function groupProvidersByCategory(
  providers: ProviderCatalogEntry[],
  categories: ProviderCategory[],
): { category: ProviderCategory; providers: ProviderCatalogEntry[] }[] {
  const order = new Map(
    PROVIDER_CATEGORY_ORDER.map((id, index) => [id, index] as const),
  );
  const sortedCategories = [...categories].sort(
    (left, right) =>
      (order.get(left.id as (typeof PROVIDER_CATEGORY_ORDER)[number]) ?? 99) -
      (order.get(right.id as (typeof PROVIDER_CATEGORY_ORDER)[number]) ?? 99),
  );
  return sortedCategories
    .map((category) => ({
      category,
      providers: providers.filter((entry) => entry.category === category.id),
    }))
    .filter((group) => group.providers.length > 0);
}
