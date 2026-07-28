/**
 * Product-safe Liquid fence demos for the Operator’s Guide catalog chapter.
 * Kept free of $lib imports so the generate script can load this module via vite-node.
 */

export type GuideLiquidDemo = {
  /** Primary fence lang (not aliases). */
  id: string;
  title: string;
  blurb: string;
  /** Full fence including opening ```lang and closing ``` */
  fence: string;
};

/** Tiny offline SVG for media demos (no network). */
const DEMO_MEDIA_SRC =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360">
      <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#1e3a5f"/><stop offset="1" stop-color="#0d9488"/>
      </linearGradient></defs>
      <rect width="640" height="360" fill="url(#g)"/>
      <text x="50%" y="50%" fill="white" font-family="system-ui,sans-serif" font-size="28"
        text-anchor="middle" dominant-baseline="middle">Library preview</text>
    </svg>`,
  );

function fence(lang: string, body: string): string {
  return ["```" + lang, body.trimEnd(), "```"].join("\n");
}

/** Primary langs only (skip action_row / chip_group aliases). */
export const GUIDE_LIQUID_PRIMARY_LANGS = [
  "card",
  "carousel",
  "actions",
  "callout",
  "section",
  "block",
  "chips",
  "media",
  "cite",
  "compare",
  "plan",
  "timeline",
  "shortlist",
  "decision",
  "brief",
  "dashboard",
  "chart",
  "report",
  "slides",
  "tabs",
  "steps",
  "accordion",
  "code",
  "tree",
  "kanban",
  "feed",
] as const;

export const GUIDE_LIQUID_CATALOG: GuideLiquidDemo[] = [
  {
    id: "card",
    title: "Card",
    blurb: "Single-entity summary. Tap to open the detail sheet when the host supports it.",
    fence: fence(
      "card",
      [
        "title: Morning notes",
        "subtitle: Library",
        "icon: book",
        "summary: Keep what matters in Library so Medousa can find it later.",
      ].join("\n"),
    ),
  },
  {
    id: "carousel",
    title: "Carousel",
    blurb: "Swipe or step through a few related cards.",
    fence: fence(
      "carousel",
      [
        "title: Today’s picks",
        "",
        "---",
        "title: Chat",
        "subtitle: Ask something",
        "icon: message-circle",
        "body: Open Chat and send a short hello.",
        "---",
        "title: Library",
        "subtitle: Save a note",
        "icon: book",
        "body: Create a note while the idea is fresh.",
      ].join("\n"),
    ),
  },
  {
    id: "actions",
    title: "Actions",
    blurb: "Compact action row (alias: `action_row`).",
    fence: fence(
      "actions",
      ["Open Chat | open-chat", "Open Library | open-library"].join("\n"),
    ),
  },
  {
    id: "callout",
    title: "Callout",
    blurb: "Toned tip / note / warn callout for emphasis.",
    fence: fence(
      "callout",
      [
        "tone: tip",
        "title: Start simple",
        "body: Most days you only need Chat, Library, and a healthy connection.",
      ].join("\n"),
    ),
  },
  {
    id: "section",
    title: "Section",
    blurb: "Labeled section with supporting prose.",
    fence: fence(
      "section",
      [
        "title: Under the chat box",
        "subtitle: Voice · Stance · Runtime",
        "---",
        "Leave these alone until a turn needs a different feel or model path.",
      ].join("\n"),
    ),
  },
  {
    id: "block",
    title: "Block",
    blurb: "Styled prose block (font, size, spacing).",
    fence: fence(
      "block",
      [
        "id: guide-styled",
        "font: serif",
        "size: lg",
        "align: left",
        "spacing: relaxed",
        "---",
        "A quieter reading block inside a note — adjust chrome in Live when you want a different look.",
      ].join("\n"),
    ),
  },
  {
    id: "chips",
    title: "Chips",
    blurb: "Compact choice or label chips (alias: `chip_group`).",
    fence: fence(
      "chips",
      [
        "- Voice | tone: accent | value: voice",
        "Stance | tone: default",
        "Runtime | tone: warn",
      ].join("\n"),
    ),
  },
  {
    id: "media",
    title: "Media",
    blurb: "Image or media embed. Vault paths and https URLs both work; this demo uses an offline SVG.",
    fence: fence(
      "media",
      [
        `src: ${DEMO_MEDIA_SRC}`,
        "alt: Sample preview",
        "caption: Replace with a vault image path or URL in your notes.",
        "ratio: 16/9",
      ].join("\n"),
    ),
  },
  {
    id: "cite",
    title: "Cite",
    blurb: "Source citation with optional quote.",
    fence: fence(
      "cite",
      [
        "title: Operator’s Guide",
        "quote: Use this manual when you want to know what a control does.",
        "source: guide",
      ].join("\n"),
    ),
  },
  {
    id: "compare",
    title: "Compare",
    blurb: "Side-by-side options with a recommendation.",
    fence: fence(
      "compare",
      [
        "title: Phone vs peer",
        "subtitle: Two different relationships",
        "recommendation: Phone",
        "",
        "| | Phone | Peer |",
        "| --- | --- | --- |",
        "| Notes | Same as your workshop | Separate workshop |",
        "| Setup | Pair QR on Wi‑Fi | Trust on the network |",
      ].join("\n"),
    ),
  },
  {
    id: "plan",
    title: "Plan",
    blurb: "Paced checklist grouped by day (or similar).",
    fence: fence(
      "plan",
      [
        "title: First hour",
        "subtitle: Prove it works",
        "grouping: day",
        "",
        "---",
        "label: Send a hello",
        "time: Now",
        "icon: message-circle",
        "body: Open Chat and send one short message.",
        "---",
        "label: Save a note",
        "time: Next",
        "icon: book",
        "body: Library → Notes — keep something worth remembering.",
      ].join("\n"),
    ),
  },
  {
    id: "timeline",
    title: "Timeline",
    blurb: "Chronological events on a vertical rail. See also Timeline layouts below for `snapshot`.",
    fence: fence(
      "timeline",
      [
        "title: Operate a Work card",
        "subtitle: Typical loop",
        "",
        "---",
        "ts: Open",
        "label: Open inspector",
        "detail: Click a card for timeline, result, and links.",
        "icon: search",
        "---",
        "ts: Act",
        "label: Act or cancel",
        "detail: Drag in-flight to cancel, or finish what’s blocked.",
        "icon: zap",
      ].join("\n"),
    ),
  },
  {
    id: "shortlist",
    title: "Shortlist",
    blurb: "Ranked picks with scores.",
    fence: fence(
      "shortlist",
      [
        "title: Where to start",
        "subtitle: Everyday path",
        "criteria: usefulness · simplicity",
        "density: comfortable",
        "",
        "---",
        "label: Chat",
        "summary: Ask and iterate",
        "score: 9.5",
        "icon: message-circle",
        "---",
        "label: Library",
        "summary: Keep durable notes",
        "score: 9.0",
        "icon: book",
      ].join("\n"),
    ),
  },
  {
    id: "decision",
    title: "Decision",
    blurb: "Weighted options with pros and cons.",
    fence: fence(
      "decision",
      [
        "title: With a brain or later?",
        "subtitle: Welcome wizard",
        "factors: answers · setup time",
        "recommendation: With a brain",
        "",
        "---",
        "label: With a brain",
        "score: 9.0",
        "pros: Can answer in Chat | Models ready",
        "cons: Needs a key or Offline download",
        "---",
        "label: Workspace only",
        "score: 7.0",
        "pros: Faster first open | Add models later",
        "cons: Chat won’t answer until models are set",
      ].join("\n"),
    ),
  },
  {
    id: "brief",
    title: "Brief",
    blurb: "One-page structured takeaway.",
    fence: fence(
      "brief",
      [
        "title: Connection check",
        "subtitle: Before you troubleshoot",
        "tone: research",
        "",
        "---",
        "heading: Look first",
        "body: Status bar should say Connected — not Offline.",
        "---",
        "heading: Then try",
        "body: Settings → Connection → Save & test, or Start / Restart on desktop.",
      ].join("\n"),
    ),
  },
  {
    id: "dashboard",
    title: "Dashboard",
    blurb: "Metric tiles at a glance.",
    fence: fence(
      "dashboard",
      [
        "title: Work columns",
        "columns: 2",
        "",
        "---",
        "label: Backlog",
        "value: Queued",
        "tone: default",
        "---",
        "label: In flight",
        "value: Running",
        "tone: accent",
        "---",
        "label: Blocked",
        "value: Needs you",
        "tone: warn",
        "---",
        "label: Done",
        "value: Finished",
        "tone: success",
      ].join("\n"),
    ),
  },
  {
    id: "chart",
    title: "Chart",
    blurb: "Data chart. This demo is `type: bar` — see Chart types for the full list.",
    fence: fence(
      "chart",
      [
        "type: bar",
        "title: Notes this week",
        "legend: bottom",
        "",
        "| Day | Count |",
        "| --- | --- |",
        "| Mon | 4 |",
        "| Tue | 6 |",
        "| Wed | 3 |",
      ].join("\n"),
    ),
  },
  {
    id: "report",
    title: "Report",
    blurb: "Narrative layout that can nest charts.",
    fence: fence(
      "report",
      [
        "title: Weekly pulse",
        "subtitle: One workshop",
        "columns: 1",
        "",
        "A short narrative above the figure.",
        "",
        "```chart",
        "type: line",
        "title: Chats started",
        "legend: bottom",
        "",
        "| Day | Chats |",
        "| --- | --- |",
        "| Mon | 2 |",
        "| Tue | 5 |",
        "| Wed | 4 |",
        "```",
      ].join("\n"),
    ),
  },
  {
    id: "slides",
    title: "Slides",
    blurb: "Lightweight deck inside a note.",
    fence: fence(
      "slides",
      [
        "title: Quick tour",
        "theme: dusk",
        "columns: 1",
        "",
        "---",
        "label: Welcome",
        "layout: hero",
        "",
        "# Medousa",
        "Chat, notes, and the tools around them.",
        "",
        "---",
        "label: Next",
        "layout: stack",
        "",
        "Open **Chat**, then keep what matters in **Library**.",
      ].join("\n"),
    ),
  },
  {
    id: "tabs",
    title: "Tabs",
    blurb: "Switch between labeled panels.",
    fence: fence(
      "tabs",
      [
        "title: Workshop relationships",
        "default: Your workshop",
        "",
        "---",
        "label: Your workshop",
        "body: Your notes and chats on this computer (or the one you connected to).",
        "---",
        "label: Phone",
        "body: Another screen into the same workshop — not a second brain.",
      ].join("\n"),
    ),
  },
  {
    id: "steps",
    title: "Steps",
    blurb: "Ordered procedure with optional status.",
    fence: fence(
      "steps",
      [
        "title: Pair a phone",
        "",
        "---",
        "label: Show the QR",
        "body: Settings → Sharing → Phone on the computer",
        "status: done",
        "icon: home",
        "---",
        "label: Scan",
        "body: Same Wi‑Fi; wait until the connection looks healthy",
        "status: current",
        "icon: globe",
        "---",
        "label: Forget later",
        "body: Revoke from the paired list when you’re done",
        "status: pending",
        "icon: lock",
      ].join("\n"),
    ),
  },
  {
    id: "accordion",
    title: "Accordion",
    blurb: "Collapsible FAQ-style panels.",
    fence: fence(
      "accordion",
      [
        "title: Quick answers",
        "multiple: true",
        "",
        "---",
        "label: Where do notes live?",
        "body: In the active **workshop** — check the status bar if things look empty.",
        "icon: book",
        "open: true",
        "---",
        "label: What does Offline mean?",
        "body: Home can’t reach the workshop yet. Try Workshop → Save & test.",
        "icon: alert-triangle",
      ].join("\n"),
    ),
  },
  {
    id: "code",
    title: "Code",
    blurb: "Syntax-highlighted snippet with a language badge.",
    fence: fence(
      "code",
      [
        "lang: markdown",
        "title: note.md",
        "---",
        "# Meeting notes",
        "",
        "- Decision: ship the phone pair flow",
        "- Next: write the Operator’s Guide tip",
      ].join("\n"),
    ),
  },
  {
    id: "tree",
    title: "Tree",
    blurb: "File or folder tree.",
    fence: fence(
      "tree",
      [
        "title: Library sketch",
        "---",
        "Notes/",
        "  Projects/",
        "    Brief.md",
        "  Inbox/",
        "Attachments/",
      ].join("\n"),
    ),
  },
  {
    id: "kanban",
    title: "Kanban",
    blurb: "Simple column board from markdown headings and tasks.",
    fence: fence(
      "kanban",
      [
        "## Backlog",
        "- [ ] Draft the brief",
        "",
        "## Doing",
        "- [ ] Gather sources in Web",
        "",
        "## Done",
        "- [x] Pair phone",
      ].join("\n"),
    ),
  },
  {
    id: "feed",
    title: "Feed",
    blurb:
      "Last-good automation output. This demo id won’t resolve in the guide — you’ll see the empty state until a real schedule writes to the feed.",
    fence: fence(
      "feed",
      [
        "id: guide-demo-digest",
        "datatype: md",
        "title: Demo digest",
        "empty: No feed output yet — wire this id to an automation schedule.",
        "refresh: load",
      ].join("\n"),
    ),
  },
];

const TIMELINE_SNAPSHOT_FENCE = fence(
  "timeline",
  [
    "title: Research day",
    "subtitle: Snapshot layout",
    "layout: snapshot",
    "",
    "---",
    "ts: Morning",
    "title: Browse",
    "meta: web",
    "body: Save useful pages to Library.",
    "icon: globe",
    "---",
    "ts: Afternoon",
    "title: Draft",
    "meta: notes",
    "body: Ask Chat for a short cited brief.",
    "icon: pencil",
    "---",
    "ts: Evening",
    "title: Keep",
    "meta: library",
    "body: File the result in Notes.",
    "icon: book",
  ].join("\n"),
);

function shieldSource(fenceSource: string): string {
  return ["````markdown", fenceSource.trimEnd(), "````"].join("\n");
}

function catalogEntryMarkdown(demo: GuideLiquidDemo): string {
  return [
    `### ${demo.title}`,
    "",
    demo.blurb,
    "",
    demo.fence.trimEnd(),
    "",
    shieldSource(demo.fence),
    "",
  ].join("\n");
}

/** Full Operator’s Guide Liquid blocks chapter markdown. */
export function buildLiquidCatalogMarkdown(): string {
  const catalogBody = GUIDE_LIQUID_CATALOG.map(catalogEntryMarkdown).join("\n");

  return [
    "# Liquid blocks",
    "",
    "**Advanced.** Liquid blocks are interactive pieces inside notes — cards, charts, plans, feeds, and more. Insert them from the Live slash menu under **Blocks**, or type the fences in Build.",
    "",
    "Related: [Vault and notes](guide:vault-notes) · [Views and environments](guide:views-environments)",
    "",
    "Each entry below shows a **live example**, then the **source** you can copy.",
    "",
    "## Catalog",
    "",
    catalogBody.trimEnd(),
    "",
    "## Chart types",
    "",
    "Set `type:` on a `chart` fence. The catalog demo above uses **bar**. Other values:",
    "",
    "`bar` · `line` · `area` · `pie` · `donut` · `radar` · `radial` · `scatter` · `combo` · `heatmap`",
    "",
    "Start from slash **Blocks → Chart** so the table skeleton matches the type.",
    "",
    "## Timeline layouts",
    "",
    "Default timeline is a vertical rail with a time gutter (see **Timeline** in the catalog). Use `layout: snapshot` for a horizontal track with peek cards:",
    "",
    TIMELINE_SNAPSHOT_FENCE.trimEnd(),
    "",
    shieldSource(TIMELINE_SNAPSHOT_FENCE),
    "",
    "## Authoring tips",
    "",
    "1. Start from slash **Blocks** so the fence skeleton is valid.",
    "2. Nest charts inside `report` when you need a narrative + visuals layout.",
    "3. Keep `feed` ids stable so badges and last-good resolve.",
    "4. Aliases: `action_row` → `actions`, `chip_group` → `chips`.",
    "5. Export PDF/Word may flatten some interactivity — check [Vault and notes](guide:vault-notes#export-and-chat-bridges).",
    "",
    "Next: [Vault and notes](guide:vault-notes).",
    "",
  ].join("\n");
}
