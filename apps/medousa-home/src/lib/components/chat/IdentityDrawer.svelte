<script lang="ts">
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { X } from "@lucide/svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { chat } from "$lib/stores/chat.svelte";
  import { identity } from "$lib/stores/identity.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    open: boolean;
    onClose?: () => void;
    onOpenFullContext?: () => void;
    variant?: "drawer" | "sheet";
  }

  let { open, onClose, onOpenFullContext, variant = "drawer" }: Props = $props();

  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  $effect(() => {
    if (open) {
      void identity.refresh({ relationshipLimit: 8 });
    }
  });

  function relationshipKind(value: unknown): string {
    if (typeof value === "string") return value;
    if (value && typeof value === "object" && "type" in value) {
      return String((value as { type?: string }).type ?? "relationship");
    }
    return "relationship";
  }

  function close() {
    layout.setIdentityDrawerOpen(false);
    onClose?.();
  }

  function dismissSheet() {
    haptic("light");
    close();
  }

  $effect(() => {
    if (!open || variant !== "sheet") return;
    return registerMobileBackHandler(() => {
      close();
      return true;
    });
  });

  $effect(() => {
    if (!open || variant !== "sheet" || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: dismissSheet });
  });
</script>

{#if open}
  {#if variant === "sheet"}
    <div
      class="mobile-sheet-backdrop mobile-sheet-peek-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) dismissSheet();
      }}
    >
      <div
        bind:this={sheetEl}
        class="mobile-sheet mobile-sheet-peek mobile-sheet-peek-tall flex flex-col"
        role="dialog"
        aria-label="Identity recall"
      >
        <header
          bind:this={headerEl}
          class="mobile-sheet-header scripts-workbench-sheet-header mobile-chat-history-header"
        >
          <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
          <div class="flex w-full items-start justify-between gap-2">
            <div class="min-w-0">
              <h2 class="text-sm font-semibold text-surface-50">Identity recall</h2>
              <p class="workshop-faint mt-0.5 truncate text-xs">
                {userProfiles.activeDisplayName}
                {#if settings.showEngineDetailsInChat}
                  · {chat.sessionId}
                {/if}
              </p>
            </div>
            <button
              type="button"
              class="btn btn-sm shrink-0 variant-ghost-surface"
              aria-label="Close identity recall"
              onclick={dismissSheet}
            >
              Done
            </button>
          </div>
        </header>
        {@render identityPanelBody()}
      </div>
    </div>
  {:else}
    <button
      type="button"
      class="absolute inset-0 z-20 bg-surface-950/50"
      aria-label="Close identity recall"
      onclick={onClose}
    ></button>

    <aside
      class="workshop-rail absolute right-0 top-0 z-30 flex h-full w-72 flex-col"
      aria-label="Identity recall"
    >
      <div class="workshop-header px-3 py-3">
        <div>
          <p class="text-sm font-semibold text-surface-100">Identity recall</p>
          <p class="workshop-faint truncate">
            {userProfiles.activeDisplayName}
            {#if settings.showEngineDetailsInChat}
              · {chat.sessionId}
            {/if}
          </p>
        </div>
        {#if onClose}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            aria-label="Close identity recall"
            onclick={close}
          >
            <X size={16} strokeWidth={1.75} />
          </button>
        {/if}
      </div>
      {@render identityPanelBody()}
    </aside>
  {/if}
{/if}

{#snippet identityPanelBody()}
  <div class="min-h-0 flex-1 overflow-y-auto p-3 text-sm">
    {#if identity.loading}
      <p class="workshop-muted">Loading recall context…</p>
    {:else if identity.error}
      <p class="text-xs text-error-400">{identity.error}</p>
    {:else if !identity.context}
      <p class="workshop-muted">No identity context available.</p>
    {:else}
      <dl class="space-y-3 text-xs">
        {#if identity.context.persona}
          <div class="workshop-inset p-3">
            <dt class="workshop-label">Persona</dt>
            <dd class="mt-1 font-medium text-surface-100">
              {identity.context.persona.display_name}
            </dd>
            <dd class="workshop-faint mt-0.5">
              {identity.context.persona.persona_id} · {identity.context.persona.status}
            </dd>
          </div>
        {/if}

        {#if identity.context.user}
          <div class="workshop-inset p-3">
            <dt class="workshop-label">You · {userProfiles.activeDisplayName}</dt>
            <dd class="mt-1 font-medium text-surface-100">
              {identity.context.user.timezone}
            </dd>
            <dd class="workshop-faint mt-0.5">
              {identity.context.user.status}
              {#if settings.showEngineDetailsInChat}
                · {identity.context.user.user_id}
              {/if}
            </dd>
          </div>
        {/if}

        {#if identity.context.contacts && identity.context.contacts.length > 0}
          <div class="workshop-inset p-3">
            <dt class="workshop-label">Contacts · {identity.context.contacts.length}</dt>
            <ul class="mt-2 space-y-1">
              {#each identity.context.contacts.slice(0, 6) as contact (contact.contact_id)}
                <li class="text-surface-200">{contact.display_name}</li>
              {/each}
            </ul>
          </div>
        {/if}

        {#if identity.context.flattened_claims && identity.context.flattened_claims.length > 0}
          <div class="workshop-inset p-3">
            <dt class="workshop-label">
              Recall claims · {identity.context.flattened_claims.length}
            </dt>
            <ul class="mt-2 space-y-2">
              {#each identity.context.flattened_claims.slice(0, 8) as claim (claim.claim_id)}
                <li>
                  <p class="leading-relaxed text-surface-200">{claim.summary}</p>
                  <p class="workshop-faint mt-0.5">
                    confidence {(claim.confidence * 100).toFixed(0)}%
                  </p>
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        {#if identity.context.relationships && identity.context.relationships.length > 0}
          <div class="workshop-inset p-3">
            <dt class="workshop-label">
              Relationships · {identity.context.relationships.length}
            </dt>
            <ul class="mt-2 space-y-1 text-surface-300">
              {#each identity.context.relationships.slice(0, 5) as rel (rel.relationship_id)}
                <li>
                  {relationshipKind(rel.relationship_kind)} · trust
                  {(rel.trust_level * 100).toFixed(0)}%
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        <p class="workshop-faint">
          Graph depth {identity.context.graph_depth_used}
        </p>

        {#if onOpenFullContext}
          <button
            type="button"
            class="workshop-text-action mt-2 text-sm"
            onclick={() => {
              close();
              onOpenFullContext();
            }}
          >
            Open in Context →
          </button>
        {/if}
        <button
          type="button"
          class="workshop-text-action mt-2 block text-sm"
          onclick={() => {
            close();
            layout.navigateDesktop("profiles");
          }}
        >
          Open Profiles →
        </button>
      </dl>
    {/if}
  </div>
{/snippet}
