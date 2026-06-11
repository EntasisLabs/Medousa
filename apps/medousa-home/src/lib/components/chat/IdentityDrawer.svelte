<script lang="ts">
  import { X } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { identity } from "$lib/stores/identity.svelte";
  import { layout } from "$lib/stores/layout.svelte";

  interface Props {
    open: boolean;
    onClose?: () => void;
    onOpenFullContext?: () => void;
    variant?: "drawer" | "sheet";
  }

  let { open, onClose, onOpenFullContext, variant = "drawer" }: Props = $props();

  $effect(() => {
    if (open) {
      void identity.refreshForSession(chat.sessionId);
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
</script>

{#if open}
  {#if variant === "sheet"}
    <div
      class="mobile-sheet-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div
        class="mobile-sheet mobile-sheet-tall flex flex-col"
        role="dialog"
        aria-label="Identity recall"
      >
        {@render identityPanel()}
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
      {@render identityPanel()}
    </aside>
  {/if}
{/if}

{#snippet identityPanel()}
  <div class="workshop-header px-3 py-3">
    <div>
      <p class="text-sm font-semibold text-surface-100">Identity recall</p>
      <p class="workshop-faint truncate">{chat.sessionId}</p>
    </div>
    {#if onClose || variant === "sheet"}
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        aria-label="Close identity recall"
        onclick={close}
      >
        {variant === "sheet" ? "Done" : ""}
        {#if variant === "drawer"}
          <X size={16} strokeWidth={1.75} />
        {/if}
      </button>
    {/if}
  </div>

  <div class="flex-1 overflow-y-auto p-3 text-sm">
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
            <dt class="workshop-label">User</dt>
            <dd class="mt-1 font-medium text-surface-100">
              {identity.context.user.user_id}
            </dd>
            <dd class="workshop-faint mt-0.5">
              {identity.context.user.timezone} · {identity.context.user.status}
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
      </dl>
    {/if}
  </div>
{/snippet}
