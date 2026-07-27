import { describe, expect, it } from "vitest";
import {
  parseDueDay,
  parseRemindersFromMarkdown,
  stripDueMarker,
} from "$lib/utils/calendarReminders";

describe("calendarReminders", () => {
  it("parses @due markers", () => {
    expect(parseDueDay("Pay rent @due(2026-08-01)")).toBe("2026-08-01");
    expect(stripDueMarker("Pay rent @due(2026-08-01)")).toBe("Pay rent");
  });

  it("extracts incomplete reminders from markdown", () => {
    const md = `---
title: Reminders
---

# Reminders

- [ ] Electric bill @due(2026-07-02)
- [x] Done already @due(2026-07-01)
- [ ] No due date
`;
    const items = parseRemindersFromMarkdown(md, "calendar/reminders.md");
    expect(items).toHaveLength(2);
    expect(items[0]).toMatchObject({
      title: "Electric bill",
      dueDay: "2026-07-02",
      completed: false,
    });
    expect(items[1]?.completed).toBe(true);
  });
});
