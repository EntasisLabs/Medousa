import { expect, it } from "vitest";
import { reviewChangeKind, typedChanges } from "./reviewChangeKinds";
it("classifies each path once with CI, tests and docs taking precedence over code extensions", () => {
  for (const [path, kind] of [["src/main.rs", "code"], ["src/tests.rs", "tests"], ["src/unit_tests.rs", "tests"], ["scripts/ci/check.sh", "ci"], [".github/actions/check/action.yml", "ci"], ["tests/main.rs", "tests"], ["src/foo.test.ts", "tests"], ["foo_test.go", "tests"], ["test_api.py", "tests"], ["docs/example.ts", "docs"], ["README.md", "docs"], [".github/workflows/test.yml", "ci"], [".gitlab-ci.yml", "ci"], ["Jenkinsfile", "ci"], [".circleci/config.yml", "ci"], ["src\\__tests__\\foo.ts", "tests"], ["package-lock.json", "other"], ["image.png", "other"]]) expect(reviewChangeKind(path)).toBe(kind);
});
it("preserves file and line totals and marks missing stats instead of inventing zeros", () => {
  const rows = typedChanges([{ path: "src/a.rs", additions: 10, deletions: 2 }, { path: "tests/a.rs", additions: 7, deletions: 0 }, { path: "docs/a.md" }]);
  expect(rows.reduce((n, r) => n + r.count, 0)).toBe(3);
  expect(rows.reduce((n, r) => n + r.additions, 0)).toBe(17);
  expect(rows.find(r => r.kind === "docs")?.known).toBe(false);
  expect(rows.find(r => r.kind === "tests")?.known).toBe(true);
});
