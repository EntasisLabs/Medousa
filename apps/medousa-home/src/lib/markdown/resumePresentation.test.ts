import { describe, expect, it } from "vitest";

import { renderMarkdown } from "./render";
import { enhanceResumeRoleMeta } from "./resumePresentation";

describe("resumePresentation", () => {
  it("splits Company | Dates after an h3 into a meta row", () => {
    const html = enhanceResumeRoleMeta(
      "<h3>Logistics Coordinator</h3><p>DriveTime | May 2026 – Present</p>",
    );
    expect(html).toContain('class="resume-role-meta"');
    expect(html).toContain('class="resume-role-org">DriveTime</span>');
    expect(html).toContain('class="resume-role-dates">May 2026 – Present</span>');
  });

  it("leaves long prose with a pipe alone", () => {
    const prose =
      "<h3>Role</h3><p>Led a program across A | B teams with a very long explanation that should not become a timeline row.</p>";
    expect(enhanceResumeRoleMeta(prose)).toBe(prose);
  });

  it("wires through renderMarkdown", () => {
    const html = renderMarkdown(
      [
        "### Logistics Coordinator",
        "",
        "DriveTime | May 2026 – Present",
        "",
        "- **Carrier & Vendor Relations:** Kept freight moving.",
      ].join("\n"),
    );
    expect(html).toContain("resume-role-meta");
    expect(html).toContain("DriveTime");
    expect(html).toContain("May 2026");
  });
});
