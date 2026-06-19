import { slugifyHeading, uniqueHeadingSlug } from "$lib/utils/headingSlug";

export { slugifyHeading, uniqueHeadingSlug };

export function plainHeadingText(html: string): string {
  return html.replace(/<[^>]+>/g, "").trim();
}
