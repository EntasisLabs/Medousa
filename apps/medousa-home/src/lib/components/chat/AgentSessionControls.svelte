<script lang="ts">
  import { tick } from "svelte";
  import { Brain, Check, ChevronDown, Cpu } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeComposerPopover } from "$lib/utils/railPopover";
  import type { AgentSessionConfigOption } from "$lib/daemon";

  interface Props {
    options: AgentSessionConfigOption[];
    disabled?: boolean;
    onChange?: (configId: string, value: unknown) => void | Promise<void>;
  }

  let { options, disabled = false, onChange }: Props = $props();
  let openId = $state<string | null>(null);
  let savingId = $state<string | null>(null);
  let rootEl = $state<HTMLDivElement | null>(null);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  const visibleOptions = $derived(
    options.filter(
      (option) =>
        option.type === "select" &&
        (option.id === "model" ||
          option.category === "model" ||
          option.id === "reasoning_effort" ||
          option.category === "thought_level"),
    ),
  );
  const activeOption = $derived(visibleOptions.find((option) => option.id === openId) ?? null);

  function currentLabel(option: AgentSessionConfigOption): string {
    const current = option.currentValue;
    return (
      option.options?.find((choice) => choice.value === current)?.name ??
      (typeof current === "string" ? current : option.name)
    );
  }

  function iconFor(option: AgentSessionConfigOption) {
    return option.id === "reasoning_effort" || option.category === "thought_level"
      ? Brain
      : Cpu;
  }

  $effect(() => {
    if (!openId || !menuEl || !triggerEl) return;
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
    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) => Boolean(rootEl?.contains(target) || menuEl?.contains(target)),
      onDismiss: () => (openId = null),
    });
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      detachDismiss();
    };
  });

  async function select(option: AgentSessionConfigOption, value: unknown) {
    if (savingId || value === option.currentValue) {
      openId = null;
      return;
    }
    savingId = option.id;
    try {
      await onChange?.(option.id, value);
      openId = null;
    } finally {
      savingId = null;
    }
  }
</script>

<div bind:this={rootEl} class="composer-turn-controls">
  {#each visibleOptions as option (option.id)}
    {@const Icon = iconFor(option)}
    <button
      type="button"
      class="composer-turn-trigger"
      class:composer-turn-trigger-open={openId === option.id}
      disabled={disabled || savingId !== null}
      aria-haspopup="listbox"
      aria-expanded={openId === option.id}
      aria-label="{option.name} — {currentLabel(option)}"
      title="{option.name} — {currentLabel(option)}"
      onclick={(event) => {
        triggerEl = event.currentTarget;
        openId = openId === option.id ? null : option.id;
      }}
    >
      <Icon size={13} strokeWidth={1.85} class="composer-turn-trigger-icon" />
      <span class="composer-turn-trigger-label">{currentLabel(option)}</span>
      <ChevronDown size={12} strokeWidth={2} class="composer-turn-trigger-chevron shrink-0" />
    </button>
  {/each}

  {#if activeOption}
    <BodyPortal>
      <div
        bind:this={menuEl}
        class="composer-anchored-menu composer-turn-menu"
        role="listbox"
        aria-label="Choose {activeOption.name.toLowerCase()}"
      >
        <header class="composer-anchored-menu-header">
          <h2 class="text-sm font-semibold text-surface-50">{activeOption.name}</h2>
        </header>
        <div class="composer-anchored-menu-body space-y-0.5">
          {#each activeOption.options ?? [] as choice}
            {@const active = choice.value === activeOption.currentValue}
            <button
              type="button"
              class="composer-turn-option"
              class:composer-turn-option-active={active}
              role="option"
              aria-selected={active}
              title={choice.description ?? choice.name}
              onclick={() => void select(activeOption, choice.value)}
            >
              <span class="composer-turn-option-label">{choice.name}</span>
              {#if active}
                <Check size={14} strokeWidth={2} class="composer-turn-option-check" />
              {/if}
            </button>
          {/each}
        </div>
      </div>
    </BodyPortal>
  {/if}
</div>
