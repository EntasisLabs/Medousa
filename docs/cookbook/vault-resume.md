# Vault resumes

Edit a CV as normal GFM markdown — headings, lists, and pipe tables — then present and export it with resume typography.

## Quick start

1. New note → template **Resume**, or set frontmatter `kind: resume`
2. Keep a portable structure (below)
3. Export PDF/Word — resume packing uses tighter margins and matrix table chrome

```md
---
kind: resume
title: Elle Smith Resume
---

# LaKenya "Elle" Smith

Maricopa, AZ | [email@example.com](mailto:email@example.com) | (555) 555-5555

## Professional summary

…

## Areas of expertise

| Logistics | Vendor relations | Reporting |
| --------- | ---------------- | --------- |
| Dispatch | Carrier ops | Dashboards |
| … | … | … |

## Professional experience

### Logistics Coordinator

DriveTime | May 2026 – Present

- **Carrier & vendor relations:** …
```

Paths under `resumes/` or `cv/` also infer `kind: resume`.

## Structure that styles well

| Block | Markdown | Presentation |
| ----- | -------- | ------------ |
| Name | `#` H1 | Large title |
| Contact | First paragraph under H1 (`\|` separators ok) | Muted meta line |
| Sections | `##` H2 | Small caps + hairline rule |
| Roles | `###` H3 then `Company \| Dates` on the next line | Org left / dates right |
| Bullets | `- **Theme:** detail` | Bold lead-in as a clean label |
| Skills matrix | 3+ column GFM table with short cells | Chip / matrix grid |
| Skills list | `- item` bullets **directly under** `##` H2 | Pill / chip cloud |

### Role timeline (still one GFM paragraph)

```md
### Logistics Coordinator

DriveTime | May 2026 – Present
```

Preview/export split that into a tidy meta row. Source stays `Company | Dates` for ATS.

### Skills list chips

A bullet list **immediately under** an H2 (Technical skills, Certifications, etc.) becomes pills. Job bullets under an H3 stay a normal list.

```md
## Technical skills

- Google Workspace
- Microsoft 365
- Asana / Trello
```

Wide tables with long narrative cells stay as normal tables; short-cell 3+ column tables get the matrix look.

## What stays portable

- No special liquid fence required for v1
- Source remains standard GFM — fine for ATS paste and other editors
- Live TipTap tables round-trip as pipe tables

## Export

When `kind: resume`:

- Compact margins (unless you already chose wide/compact explicitly)
- No page-break-before every H2
- Job blocks (`###` + `Company | Dates` + bullets) stay together across pages when they fit
- No duplicate note-title H1 above the name on the page
- Explicit `•` bullets (html2canvas-safe) and table-layout role meta (org left / dates right)
- Matrix cells print as light bordered chips

Related: [Vault & library](vault-and-library.md) · [Liquid markdown](liquid-markdown.md)
