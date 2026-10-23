<script lang="ts">
  import { onMount } from "svelte";
  import { randomUuid } from "$lib/utils/randomUuid";
  import {
    ArrowUp,
    ArrowUpRight,
    Check,
    Scan,
    Square,
    X,
    ChevronDown,
  } from "@lucide/svelte";
  import SpotlightTargetPicker from "$lib/components/layout/SpotlightTargetPicker.svelte";
  import type { ConversationTarget } from "$lib/spotlight/conversationTargets";
  import type { CreativeProjectLease } from "$lib/create/creativeWorkspace.svelte";
  import { nodeName, editableNode } from "$lib/create/creativeScene";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  let {
    lease,
    selected,
    disabled = false,
    onerror,
    capture,
    previewError = null,
  }: {
    lease: CreativeProjectLease;
    selected: string[];
    disabled?: boolean;
    onerror: (error: unknown) => void;
    capture: (revision: number, ids: string[]) => Promise<string>;
    previewError?: string | null;
  } = $props();
  let revision = $state(0),
    picker = $state(false);
  let askInput = $state<HTMLInputElement>();
  const pickerId = `create-${randomUuid()}`;
  const draftState = $derived(lease.state),
    jobs = $derived(lease.state.jobs),
    job = $derived.by(() => {
      void revision;
      return jobs.job;
    });
  const busy = $derived.by(() => {
      void revision;
      return jobs.busy;
    }),
    executing = $derived.by(() => {
      void revision;
      return jobs.executing;
    }),
    pending = $derived.by(() => {
      void revision;
      return jobs.pending;
    });
  const doc = $derived.by(() => {
    void revision;
    return lease.session.document;
  });
  const selectedNode = $derived(
    doc?.scene.nodes.find((n) => n.id === selected[0]),
  );
  const eligible = $derived(
    Boolean(
      doc &&
      selected.length &&
      selected.every((id) => {
        const node = doc.scene.nodes.find((n) => n.id === id);
        return node && editableNode(node, doc.scene.nodes);
      }),
    ),
  );
  const context = $derived(
    selected.length === 1 && selectedNode
      ? nodeName(selectedNode)
      : `${selected.length} objects`,
  );
  const jobError = $derived.by(() => {
    void revision;
    return jobs.error;
  });
  const preparing = $derived.by(() => { void revision; return jobs.preparing; });
  const thread = $derived.by(() => { void revision; return jobs.thread; });
  const pendingDecision = $derived.by(() => {
    void revision;
    return jobs.pendingDecision;
  });
  const stale = $derived(
    job?.status === "stale" ||
      Boolean(
        job?.result &&
        doc &&
        job.result.expected_revision !== doc.revision &&
        job.status !== "accepted",
      ),
  );
  const targets = $derived<ConversationTarget[]>([
    {
      id: "new:medousa",
      label: "Medousa",
      description: "Create a proposal for this selection",
      group: "Start new",
      scope: lease.api.scope,
      kind: "new",
      runtime: "medousa",
    },
  ]);
  onMount(() => {
    const a = jobs.subscribe(() => revision++),
      b = lease.session.subscribe(() => revision++);
    return () => {
      a();
      b();
    };
  });
  async function submit() {
    if (!doc || disabled || busy || executing || pending || !eligible) return;
    try {
      const base = doc.revision, ids = [...selected];
      const refining = Boolean(draftState.preview && (job?.status === "ready" || job?.status === "stale") && !previewError);
      await jobs.start(draftState.draft, base, ids, () => capture(base, ids), refining);
      draftState.preview = false;
      draftState.draft = "";
    } catch (e) {
      onerror(e);
    }
  }
  async function openConversation() {
    if (!job || activeWorkshopId() !== lease.api.scope) return;
    const { layout } = await import("$lib/runtime/layout.svelte");
    if (!layout.isMobile) {
      shellTabs.openChat(job.session_id, { title: "Creative session" });
      return;
    }
    const { chat } = await import("$lib/stores/chat.svelte");
    if (activeWorkshopId() !== lease.api.scope) return;
    await chat.switchSession(job.session_id);
    if (activeWorkshopId() === lease.api.scope)
      layout.setMobileTab("chat", { bump: true });
  }
  async function decide(accept: boolean) {
    if (!doc) return;
    try {
      await jobs.decide(accept, doc.revision);
      if (accept || jobs.job?.status === "cancelled")
        draftState.preview = false;
    } catch (e) {
      onerror(e);
    }
  }
</script>

<div class="create-ask" aria-label="Ask about selected work">
  <div class="create-ask-context" title="Requests include a canvas snapshot and the selected source images.">
    <Scan size={12} /><span
      >{selected.length
        ? `${context} selected`
        : "Select an object or frame to work with Medousa"}</span
    >
  </div>
  {#if thread.length}<details class="create-ask-thread">
      <summary>Conversation · {thread.length}{thread.length === 32 ? " recent" : ""} {thread.length === 1 ? "request" : "requests"}</summary>
      {#each thread as previous (previous.request.id)}
        <p><strong>You</strong> {previous.request.prompt}</p>
        <p><strong>Medousa</strong> {previous.result?.label ?? previous.error ?? previous.status}</p>
      {/each}
    </details>{/if}
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void submit();
    }}
  >
    <button
      type="button"
      class="create-target"
      aria-label="Choose creative assistant"
      aria-expanded={picker}
      onclick={() => {
        picker = !picker;
        askInput?.focus();
      }}>@Medousa<ChevronDown size={11} /></button
    >
    <input
      aria-label="Ask about selected work"
      bind:this={askInput}
      role="combobox"
      aria-autocomplete="list"
      aria-controls={picker ? `${pickerId}-targets` : undefined}
      aria-expanded={picker}
      aria-activedescendant={picker ? `${pickerId}-target-0` : undefined}
      placeholder={job ? "Keep refining this direction…" : "Make this feel a little more playful…"}
      bind:value={draftState.draft}
      disabled={busy || executing || pending}
      maxlength="16000"
      onkeydown={(e) => {
        if (
          picker &&
          ["Enter", "Escape", "ArrowDown", "ArrowUp"].includes(e.key)
        ) {
          e.preventDefault();
          if (e.key === "Enter" || e.key === "Escape") picker = false;
          return;
        }
        if (e.key === "@" && !draftState.draft) {
          e.preventDefault();
          picker = true;
        }
      }}
    />
    {#if executing}<button
        type="button"
        class="create-send"
        aria-label="Cancel creative request"
        disabled={busy}
        onclick={() => void decide(false)}><Square size={12} /></button
      >
    {:else}<button
        type="submit"
        class="create-send"
        aria-label="Create a proposal"
        disabled={disabled ||
          busy ||
          pending ||
          !draftState.draft.trim() ||
          !eligible}><ArrowUp size={14} /></button
      >{/if}
  </form>
  {#if picker}<div class="create-target-picker">
      <SpotlightTargetPicker
        {targets}
        index={0}
        loading={false}
        error={null}
        onSelect={() => {
          picker = false;
          askInput?.focus();
        }}
        onHighlight={() => {}}
        idPrefix={pickerId}
        inline
      />
    </div>{/if}
  {#if job || pending || jobError || preparing}
    <div class="create-ask-result" aria-live="polite">
      {#if preparing}<span>Capturing selection and image references…</span>
      {:else if pending && busy}<span>Sending request…</span>
      {:else if pending}<span
          >Your request is saved here. Retry to confirm it was received.</span
        ><button
          type="button"
          disabled={busy}
          onclick={() =>
            void jobs
              .retry()
              .then(() => (draftState.draft = ""))
              .catch(onerror)}>Retry request</button
        >
      {:else if executing}<span class="create-progress"
          >Working on your selection…</span
        >
      {:else if job?.result && (job.status === "ready" || job.status === "stale")}
        <div class="create-result-heading">
          <span>Medousa · {stale ? "Canvas updated" : "Preview"}</span><button
            type="button"
            disabled={busy || disabled}
            aria-label="Dismiss proposal"
            onclick={() => void decide(false)}><X size={12} /></button
          >
        </div>
        <p>{job.result.label}</p>
        {#if previewError}<p class="create-ask-notice">{previewError}</p>
        {:else if stale}<p class="create-ask-notice">Keeps your latest canvas edits. Applying checks for conflicts again.</p>{/if}
        <div class="create-variants">
            <button
              type="button"
              class:chosen={!draftState.preview}
              onclick={() => (draftState.preview = false)}>Original</button
            ><button
              type="button"
              class:chosen={draftState.preview}
              disabled={Boolean(previewError)}
              onclick={() => (draftState.preview = true)}>Preview</button
            ><button
              type="button"
              disabled={Boolean(previewError)}
              onclick={() => { draftState.preview = true; askInput?.focus(); }}
              >Refine preview</button
            ><button
              type="button"
              class="create-apply"
              disabled={busy || disabled || Boolean(previewError)}
              onclick={() => void decide(true)}
              >Use this<Check size={12} /></button
            >
          </div>
      {:else if job?.status === "accepted"}<span
          ><Check size={12} />Applied to the canvas. You can undo this change.</span
        >
      {:else if job?.status === "failed" || job?.status === "interrupted"}<p>
          {job.error ||
            "This request couldn’t finish. Your original is still here."}
        </p>
      {:else if job?.status === "cancelled"}<span
          >Request cancelled. Your original is still here.</span
        >{/if}
      {#if jobError}<p class="create-ask-notice">{jobError}</p>
        {#if pendingDecision}<button
            type="button"
            disabled={busy || disabled}
            onclick={() => void decide(pendingDecision!.accept)}
            >Retry decision</button
          >{:else if !pending}<button
            type="button"
            disabled={busy}
            onclick={() => void jobs.refresh()}>Check again</button
          >{/if}{/if}
      {#if job}<button
          type="button"
          class="create-conversation"
          disabled={activeWorkshopId() !== lease.api.scope}
          onclick={() => void openConversation().catch(onerror)}
          >Open conversation<ArrowUpRight size={11} /></button
        ><button type="button" class="create-conversation" disabled={busy || executing || pending || Boolean(pendingDecision)}
          onclick={() => { try { jobs.newConversation(); draftState.preview = false; askInput?.focus(); } catch (e) { onerror(e); } }}>New conversation</button>{/if}
    </div>{/if}
</div>

<style>
  .create-ask {
    width: min(100%, 560px);
    margin: 0 auto;
    background: var(--create-raised);
    border-radius: 10px;
    flex: none;
  }
  .create-ask-context {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 13px 0;
    font-size: 11px;
    color: var(--create-dim);
  }
  .create-ask form {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px;
  }
  .create-ask button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border-radius: 6px;
    font-size: 11px;
    padding: 5px 8px;
    white-space: nowrap;
  }
  .create-ask button:hover {
    background: var(--create-soft);
  }
  .create-target {
    background: var(--create-soft);
    color: var(--create-text);
  }
  .create-ask input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 0;
    color: var(--create-text);
    font: inherit;
    font-size: 12px;
    padding: 7px 4px;
    outline: none;
  }
  .create-ask input::placeholder {
    color: var(--create-dim);
  }
  .create-send {
    border-radius: 50% !important;
    width: 27px;
    height: 27px;
    padding: 5px !important;
    background: var(--create-active) !important;
    color: var(--create-active-text);
    flex: none;
  }
  .create-ask button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .create-ask-result {
    padding: 0 13px 11px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--create-text);
  }
  .create-ask-result > span {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .create-result-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    color: var(--create-dim);
  }
  .create-ask-result p {
    margin: 4px 0 8px;
  }
  .create-variants {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .create-variants button {
    background: var(--create-soft);
  }
  .create-variants button.chosen {
    box-shadow: inset 0 0 0 1px var(--create-dim);
  }
  .create-variants .create-apply {
    margin-left: auto;
    background: var(--create-active);
    color: var(--create-active-text);
  }
  .create-ask .create-conversation {
    margin-top: 7px;
    padding-left: 0;
    color: var(--create-dim);
    font-size: 10px;
  }
  .create-ask-notice {
    color: var(--create-dim);
    font-size: 11px;
  }
  .create-target-picker :global(.command-spotlight-row) {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 9px;
    text-align: left;
    padding: 8px;
    border-radius: 6px;
  }
  .create-target-picker :global(.command-spotlight-row-copy) {
    display: grid;
    gap: 3px;
    flex: 1;
  }
  .create-target-picker :global(.command-spotlight-row-title) {
    font-size: 12px;
  }
  .create-target-picker :global(.command-spotlight-row-subtitle),
  .create-target-picker :global(.command-spotlight-row-meta),
  .create-target-picker :global(.command-spotlight-section-label) {
    font-size: 10px;
    color: var(--create-dim);
  }
  .create-target-picker :global(.command-spotlight-section-label) {
    padding: 4px 8px;
  }
  .create-target-picker :global(.command-spotlight-row-active) {
    background: var(--create-soft);
  }
  .create-target-picker {
    padding: 4px 8px 8px;
  }
  .create-target-picker :global(.command-spotlight-target-picker) {
    max-height: 150px;
  }
  .create-progress {
    color: var(--create-dim);
  }
  .create-ask-thread { padding: 8px 13px 0; font-size: 11px; color: var(--create-dim); max-height: 150px; overflow: auto; }
  .create-ask-thread p { margin: 7px 0; overflow-wrap: anywhere; }
  .create-ask-thread strong { font-weight: 500; color: var(--create-text); margin-right: 5px; }
  @media (max-width: 600px) {
    .create-ask input {
      font-size: 16px;
    }
    .create-target {
      font-size: 10px !important;
    }
  }
</style>
