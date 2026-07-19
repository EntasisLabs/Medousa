/**
 * Presentation-only HTML polish for resume-shaped GFM.
 * Source stays portable: `Company | Dates` under a role heading.
 */

import { escapeHtml } from "./escape";

function stripTags(html: string): string {
  return html
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/**
 * After an H3, a single-line `Org | Dates` paragraph becomes a meta row
 * (org left, dates right). Leaves links/images alone.
 */
export function enhanceResumeRoleMeta(html: string): string {
  return html.replace(
    /(<h3\b[^>]*>[\s\S]*?<\/h3>)\s*<p(?:\s[^>]*)?>([\s\S]*?)<\/p>/gi,
    (match, heading: string, inner: string) => {
      if (/<(?:a|img|ul|ol|table|pre|code)\b/i.test(inner)) return match;
      const plain = stripTags(inner);
      const pipe = plain.indexOf("|");
      if (pipe <= 0 || pipe !== plain.lastIndexOf("|")) return match;
      const org = plain.slice(0, pipe).trim();
      const dates = plain.slice(pipe + 1).trim();
      if (!org || !dates) return match;
      // Reject accidental pipes in long prose.
      if (plain.length > 96 || org.length > 64 || dates.length > 48) return match;
      return `${heading}<p class="resume-role-meta"><span class="resume-role-org">${escapeHtml(org)}</span><span class="resume-role-dates">${escapeHtml(dates)}</span></p>`;
    },
  );
}

export function enhanceResumePresentation(html: string): string {
  return enhanceResumeRoleMeta(html);
}
