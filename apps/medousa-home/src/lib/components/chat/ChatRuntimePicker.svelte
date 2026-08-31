<script lang="ts">
  import { tick } from "svelte";
  import { Bot, Check, ChevronDown, LogIn, MousePointer2, Sparkles, Terminal } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import {
    agentRuntimeLabel,
    type ChatAgentRuntime,
  } from "$lib/utils/sessionAgentRuntime";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeComposerPopover } from "$lib/utils/railPopover";
  import { accountConnections } from "$lib/stores/accountConnections.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { accountIdForRuntime } from "$lib/utils/accountConnections";

  interface Props {
    value: ChatAgentRuntime;
    disabled?: boolean;
    onChange?: (value: ChatAgentRuntime) => void;
  }

  let { value, disabled = false, onChange }: Props = $props();

  const OPTIONS: {
    id: ChatAgentRuntime;
    hint: string;
  }[] = [
    { id: "medousa", hint: "Native Medousa turns" },
    { id: "cursor", hint: "External Cursor agent" },
    { id: "codex", hint: "ChatGPT-backed agent + chat" },
    { id: "hermes", hint: "External Hermes agent" },
  ];

  $effect(() => {
    void accountConnections.refresh();
  });

  /** Signed-out external runtimes stay visible but locked with a sign-in CTA. */
  function lockedFor(runtime: ChatAgentRuntime): boolean {
    if (runtime === "medousa") return false;
    const account = accountIdForRuntime(runtime);
    const info = accountConnections.connection(account);
    if (!info) return false;
    if (!info.binaryPresent) return true;
    return info.authStatus === "signed_out";
  }

  function lockHint(runtime: ChatAgentRuntime): string | null {
    if (runtime === "medousa" || !lockedFor(runtime)) return null;
    const account = accountIdForRuntime(runtime);
    const info = accountConnections.connection(account);
    if (info && !info.binaryPresent) {
      if (runtime === "codex") return "Install Codex CLI — Settings → Connections";
      if (runtime === "hermes") return "Install Hermes — Settings → Connections";
      return "Install Cursor CLI — Settings → Connections";
    }
    return "Sign in — Settings → Connections";
  }

  function openConnections() {
    open = false;
    settingsNav.setActiveSection("connections");
  }

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  const label = $derived(agentRuntimeLabel(value));

  $effect(() => {
    if (!open || !menuEl || !triggerEl) return;

    let frame = 0;
    const place = () => {
      if (!menuEl || !triggerEl) return;
      placeComposerPopover(triggerEl, menuEl);
      frame = window.requestAnimationFrame(() => {
        if (menuEl && triggerEl) placeComposerPopover(triggerEl, menuEl);
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);

    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) =>
        Boolean(menuEl?.contains(target) || triggerEl?.contains(target)),
      onDismiss: () => {
        open = false;
      },
    });

    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
      detachDismiss();
    };
  });

  function pick(next: ChatAgentRuntime) {
    if (next === value) {
      open = false;
      return;
    }
    onChange?.(next);
    open = false;
  }
</script>

{#snippet runtimeIcon(runtime: ChatAgentRuntime, size = 13)}
  {#if runtime === "cursor"}
    <MousePointer2 {size} strokeWidth={1.85} class="shrink-0 opacity-70" />
  {:else if runtime === "codex"}
    <Terminal {size} strokeWidth={1.85} class="shrink-0 opacity-70" />
  {:else if runtime === "hermes"}
    <Sparkles {size} strokeWidth={1.85} class="shrink-0 opacity-70" />
  {:else}
    <Bot {size} strokeWidth={1.85} class="shrink-0 opacity-70" />
  {/if}
{/snippet}

<div class="chat-runtime-picker">
  <button
    bind:this={triggerEl}
    type="button"
    class="chat-runtime-trigger"
    class:chat-runtime-trigger-open={open}
    {disabled}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label="Agent runtime — {label}"
    title="Who runs this turn"
    onclick={() => {
      if (!disabled) open = !open;
    }}
  >
    {@render runtimeIcon(value)}
    <span class="chat-runtime-trigger-label">{label}</span>
    <ChevronDown size={12} strokeWidth={2} class="chat-runtime-trigger-chevron shrink-0" />
  </button>

  {#if open}
    <!-- Portal out of the presence-dock transform / overflow stack so fixed placement is viewport-true. -->
    <BodyPortal>
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        bind:this={menuEl}
        class="composer-anchored-menu chat-runtime-menu"
        role="listbox"
        aria-label="Choose agent runtime"
      >
        <header class="composer-anchored-menu-header">
          <div class="min-w-0">
            <h2 class="text-sm font-semibold text-surface-50">Runtime</h2>
            <p class="workshop-faint mt-0.5 text-xs">Who runs this turn</p>
          </div>
        </header>
        <div class="composer-anchored-menu-body space-y-0.5">
          {#each OPTIONS as option (option.id)}
            {@const locked = lockedFor(option.id)}
            <button
              type="button"
              class="chat-runtime-option"
              class:chat-runtime-option-active={value === option.id}
              class:chat-runtime-option-locked={locked}
              role="option"
              aria-selected={value === option.id}
              aria-disabled={locked}
              onclick={() => {
                if (locked) {
                  openConnections();
                  return;
                }
                pick(option.id);
              }}
            >
              {@render runtimeIcon(option.id, 14)}
              <span class="min-w-0 flex-1 text-left">
                <span class="block text-[13px] font-medium text-surface-100"
                  >{agentRuntimeLabel(option.id)}</span
                >
                <span class="workshop-faint mt-0.5 block text-[11px]">
                  {locked ? lockHint(option.id) : option.hint}
                </span>
              </span>
              {#if locked}
                <LogIn size={13} strokeWidth={2} class="shrink-0 text-content-link" />
              {:else if value === option.id}
                <Check size={14} strokeWidth={2} class="shrink-0 text-content-link" />
              {/if}
            </button>
          {/each}
        </div>
      </div>
    </BodyPortal>
  {/if}
</div>
