<script lang="ts">
  import { Plus } from "@lucide/svelte";
  import { onMount, tick } from "svelte";
  import AutomationCreateForm from "$lib/components/automations/AutomationCreateForm.svelte";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import type { AutomationDeliveryMode } from "$lib/types/recurring";
  import { browserTimezone } from "$lib/utils/friendlySchedule";
  import { placeToolbarPopover } from "$lib/utils/railPopover";
  import "$lib/components/skills/agentEditor.css";

  interface Props {
    mobile?: boolean;
    /** Open the new schedule in an LME tab after create. */
    lmeHosted?: boolean;
    /** Dock Plus (rail) vs primary header button. */
    trigger?: "dock" | "primary";
  }

  let {
    mobile = false,
    lmeHosted = false,
    trigger = "dock",
  }: Props = $props();

  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();

  let createTitle = $state("");
  let createPrompt = $state("");
  let createCron = $state("0 9 * * *");
  let createTimezone = $state(browserTimezone());
  let createManuscript = $state<string | undefined>(undefined);
  let createDeliveryMode = $state<AutomationDeliveryMode>("in_app");
  let createTelegramChatId = $state("");

  const open = $derived(automationDraft.showCreate);

  function placePanel() {
    if (!triggerEl || !menuEl) return;
    placeToolbarPopover(triggerEl, menuEl, {
      prefer: trigger === "dock" ? "above" : "below",
      width: 22 * 16,
      maxHeightRatio: 0.82,
    });
  }

  function setOpen(next: boolean) {
    if (next) {
      if (!automationDraft.showCreate) automationDraft.openCreate();
    } else {
      automationDraft.clearCreate();
    }
  }

  function toggleOpen(event: MouseEvent) {
    event.stopPropagation();
    setOpen(!open);
  }

  async function submitCreate() {
    const response = await automations.register({
      display_name: createTitle.trim() || undefined,
      prompt: createPrompt.trim() || "Scheduled task",
      cron_expr: createCron.trim() || "0 9 * * *",
      manuscript_id: createManuscript,
      timezone: createTimezone.trim() || "UTC",
      model_hint: runtime.model,
      execution_mode: "agent_turn",
      delivery_mode: createDeliveryMode,
      telegram_chat_id: createTelegramChatId,
    });
    automationDraft.clearCreate();
    if (lmeHosted && response.recurring_id) {
      const entry = automations.definitions.find(
        (row) => row.recurring_id === response.recurring_id,
      );
      lmeWorkspace.openSchedule(
        response.recurring_id,
        entry ? automations.labelFor(entry) : createTitle.trim() || "Schedule",
      );
    }
  }

  $effect(() => {
    if (!automationDraft.showCreate || !automationDraft.createDraft) return;
    const draft = automationDraft.createDraft;
    createTitle = draft.display_name ?? "";
    createPrompt = draft.prompt;
    createCron = draft.cron_expr;
    createTimezone = draft.timezone ?? browserTimezone();
    createManuscript = draft.manuscript_id;
    createDeliveryMode = draft.delivery_mode ?? "in_app";
    createTelegramChatId = draft.telegram_chat_id ?? "";
  });

  onMount(() => {
    const onDocPointerDown = (event: PointerEvent) => {
      if (!open) return;
      const path = event.composedPath();
      if (
        (menuEl && path.includes(menuEl)) ||
        (triggerEl && path.includes(triggerEl))
      ) {
        return;
      }
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && open) setOpen(false);
    };
    document.addEventListener("pointerdown", onDocPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDocPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  });

  $effect(() => {
    if (!open || !triggerEl || !menuEl) return;
    let frame = 0;
    const place = () => {
      if (!triggerEl || !menuEl) return;
      placePanel();
      frame = window.requestAnimationFrame(() => {
        if (triggerEl && menuEl) placePanel();
      });
    };
    void tick().then(place);
    const onResize = () => place();
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onResize, true);
    const vv = window.visualViewport;
    vv?.addEventListener("resize", onResize);
    vv?.addEventListener("scroll", onResize);
    const ro = new ResizeObserver(() => place());
    ro.observe(menuEl);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onResize, true);
      vv?.removeEventListener("resize", onResize);
      vv?.removeEventListener("scroll", onResize);
      ro.disconnect();
    };
  });
</script>

<div class="agent-editor-popover relative shrink-0">
  {#if trigger === "primary"}
    <button
      bind:this={triggerEl}
      type="button"
      class="btn btn-sm shrink-0 variant-filled-primary"
      aria-haspopup="dialog"
      aria-expanded={open}
      onclick={toggleOpen}
    >
      + New schedule
    </button>
  {:else}
    <button
      bind:this={triggerEl}
      type="button"
      class="vault-dock-icon-btn {open ? 'vault-dock-icon-btn-active' : ''}"
      aria-label="New schedule"
      title="New"
      aria-haspopup="dialog"
      aria-expanded={open}
      onclick={toggleOpen}
    >
      <Plus size={16} strokeWidth={1.75} />
    </button>
  {/if}

  {#if open}
    <div
      bind:this={menuEl}
      class="agent-editor-popover-panel agent-editor-popover-panel-create"
      role="dialog"
      aria-label="New schedule"
      onpointerdown={(event) => event.stopPropagation()}
    >
      <div class="agent-editor-popover-head schedule-create-head">
        <div class="min-w-0">
          <p class="schedule-create-head-title">New schedule</p>
        </div>
      </div>
      <div class="agent-editor-popover-body min-h-0 flex-1 overflow-y-auto px-3.5 py-2.5">
        <AutomationCreateForm
          {mobile}
          compact
          bind:title={createTitle}
          bind:prompt={createPrompt}
          bind:cronExpr={createCron}
          bind:timezone={createTimezone}
          bind:deliveryMode={createDeliveryMode}
          bind:telegramChatId={createTelegramChatId}
          manuscript={createManuscript}
          registering={automations.registering}
          onCancel={() => setOpen(false)}
          onSubmit={submitCreate}
          onLayoutChange={() => {
            requestAnimationFrame(() => placePanel());
          }}
        />
      </div>
    </div>
  {/if}
</div>
