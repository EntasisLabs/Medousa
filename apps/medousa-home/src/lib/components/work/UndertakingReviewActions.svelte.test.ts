/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import UndertakingReviewActions from "./UndertakingReviewActions.svelte";

let component: ReturnType<typeof mount>;

afterEach(async () => {
  if (component) await unmount(component);
  document.body.replaceChildren();
});

function render(overrides = {}) {
  const props = {
    attached: false, busy: false, canApprove: true,
    allowReview: true, allowApply: false,
    sourceBranch: "main", reviewedBranch: "work/change",
    actionLabel: "Apply to main",
    integrationStrategy: "fast_forward_only" as const,
    onRequestChanges: vi.fn(),
    onApprove: vi.fn(), onApply: vi.fn(),
    ...overrides,
  };
  component = mount(UndertakingReviewActions, { target: document.body, props });
  flushSync();
  return props;
}

it("records approval before offering to apply unapproved changes", async () => {
  const props = render();
  document.querySelector<HTMLButtonElement>('button[aria-label="Forge review actions"]')!.click();
  flushSync(); await tick(); flushSync();
  const approve = [...document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find(button => button.textContent?.includes("Approve changes"))!;
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


it("changes the after-approval strategy without recording approval or applying changes", async () => {
  const props = render();
  document.querySelector<HTMLButtonElement>('button[aria-label="Forge review actions"]')!.click();
  flushSync(); await tick(); flushSync();
  const choices = [...document.querySelectorAll<HTMLButtonElement>('[role="menuitemcheckbox"]')];
  expect(choices).toHaveLength(2);
  expect(choices[0].getAttribute("aria-checked")).toBe("true");
  choices[1].click(); flushSync(); await tick(); flushSync();
  document.querySelector<HTMLButtonElement>('button[aria-label="Forge review actions"]')!.click();
  flushSync(); await tick(); flushSync();
  expect(document.querySelectorAll('[role="menuitemcheckbox"]')[1].getAttribute("aria-checked")).toBe("true");
  expect(props.onApprove).not.toHaveBeenCalled();
  expect(props.onApply).not.toHaveBeenCalled();
});

it("disables the review menu while another action is running", () => {
  const props = render({ busy: true });
  const trigger = document.querySelector<HTMLButtonElement>('button[aria-label="Forge review actions"]')!;
  expect(trigger.disabled).toBe(true);
  trigger.click();
  expect(document.querySelector('[role="menuitem"]')).toBeNull();
  expect(props.onApprove).not.toHaveBeenCalled();
});
