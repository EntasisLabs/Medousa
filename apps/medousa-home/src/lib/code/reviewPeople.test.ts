import { expect, it } from "vitest";
import { reviewPeople } from "./reviewPeople";
it("keeps decisions after comments, respects dismissals and treats re-requests as pending", () => {
  const people = reviewPeople([
    { id: "a", name: "A", state: "approved" }, { id: "a", name: "A", state: "commented" },
    { id: "b", name: "B", state: "changes_requested" }, { id: "b", name: "B", state: "dismissed" },
    { id: "c", name: "C", state: "approved" },
  ], ["c", "d"]);
  expect(people.map(p => [p.id, p.state])).toEqual([["a", "approved"], ["b", "dismissed"], ["c", "pending"], ["d", "pending"]]);
});
