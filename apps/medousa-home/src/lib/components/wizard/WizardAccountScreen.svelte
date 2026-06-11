<script lang="ts">
  import { wizard } from "$lib/stores/wizard.svelte";
</script>

<div class="flex h-full flex-col">
  <button
    type="button"
    class="workshop-text-action self-start text-sm"
    disabled={wizard.busy}
    onclick={() => void wizard.back()}
  >
    ← Back
  </button>

  <p class="mt-4 text-[11px] font-semibold uppercase tracking-wide text-primary-300">Step 2 of 3</p>
  <h2 class="mt-2 text-2xl font-semibold text-surface-50">Create your Medousa account</h2>
  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    Your account links devices and unlocks managed AI when you're ready. No cloud storage unless
    you say so — your brain stays on this machine.
  </p>

  <div class="mt-6 space-y-3 rounded-xl border border-surface-500/35 bg-surface-950/40 p-5">
    <p class="text-sm text-surface-400">
      Magic link and Sign in with Apple/Google arrive in Phase E. Everything works locally if you
      skip.
    </p>
    <button type="button" class="btn variant-soft w-full min-h-11 opacity-60" disabled>
      Send magic link (coming soon)
    </button>
  </div>

  <blockquote class="mt-6 border-l-2 border-surface-500/50 pl-4 text-sm text-surface-400">
    Your data stays yours. Creating an account only links your devices. We don't train on your
    data. We don't sell your data.
  </blockquote>

  <div class="mt-auto flex flex-wrap items-center justify-between gap-3 pt-8">
    <button
      type="button"
      class="btn variant-ghost min-h-11"
      disabled={wizard.busy}
      onclick={() => void wizard.skipCurrent()}
    >
      Skip — all features work locally
    </button>
    <button
      type="button"
      class="btn variant-filled-primary min-h-11 px-6 opacity-60"
      disabled
      title="Available when Medousa Cloud ships"
    >
      Continue
    </button>
  </div>
</div>
