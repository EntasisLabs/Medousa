/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import UndertakingReviewActions from "./UndertakingReviewActions.svelte";

let component: ReturnType<typeof mount>;

afterEach(async () => {
  if (component) await unmount(component);
  document.body.replaceChildren();
});

function render(overrides = {}) {
  const props = {
    attached: false, busy: false, canApprove: true,
    allowContinue: true, allowReview: true, allowApply: false,
    sourceBranch: "main", reviewedBranch: "work/change",
    actionLabel: "Apply to main",
    integrationStrategy: "fast_forward_only" as const,
    onContinue: vi.fn(), onRequestChanges: vi.fn(),
    onApprove: vi.fn(), onApply: vi.fn(),
    ...overrides,
  };
  component = mount(UndertakingReviewActions, { target: document.body, props });
  flushSync();
  return props;
}

it("records approval before offering to apply unapproved changes", () => {
  const props = render();
  const approve = document.querySelector<HTMLButtonElement>('button[aria-label="Approve changes · Apply to main"]')!;
  expect(approve).not.toBeNull();
  expect(document.querySelector('button[aria-label="Apply to main"]')).toBeNull();
  approve.click();
  expect(props.onApprove).toHaveBeenCalledOnce();
  expect(props.onApply).not.toHaveBeenCalled();
});

it.each([
  { attached: false, actionLabel: "Apply to main" },
  { attached: false, actionLabel: "Keep work/change" },
  { attached: true, actionLabel: "Keep changes here" },
])("offers $actionLabel when approval enables apply alongside review", ({ attached, actionLabel }) => {
  const props = render({ attached, actionLabel, allowApply: true });
  const apply = document.querySelector<HTMLButtonElement>(`button[aria-label="${actionLabel}"]`)!;
  expect(apply).not.toBeNull();
  expect(document.querySelector("select")).toBeNull();
  expect(document.body.textContent).not.toContain("Approve");
  expect(props.onApply).not.toHaveBeenCalled();
  apply.click();
  expect(props.onApply).toHaveBeenCalledOnce();
  expect(props.onApprove).not.toHaveBeenCalled();
});

it("disables applying while a project action is running", () => {
  const props = render({ allowApply: true, busy: true });
  const apply = document.querySelector<HTMLButtonElement>('button[aria-label="Apply to main"]')!;
  expect(apply.disabled).toBe(true);
  apply.click();
  expect(props.onApply).not.toHaveBeenCalled();
});

it("offers no decision actions once review and apply are unavailable", () => {
  render({ allowReview: false, allowApply: false });
  expect(document.querySelector("button")).toBeNull();
});
