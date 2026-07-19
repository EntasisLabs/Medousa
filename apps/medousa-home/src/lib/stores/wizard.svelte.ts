import {
  advanceWizard,
  beginWizardRerun,
  bootstrapWizard,
  completeWizard,
} from "$lib/utils/wizardApi";
import { layout, saveLastSurface } from "$lib/stores/layout.svelte";
import {
  applyWizardScreen1,
  startEngine,
  type WizardApplyScreen1Request,
} from "$lib/utils/providersApi";
import { isTauriMobilePlatform } from "$lib/platform";
import { isTauri } from "$lib/window";
import { completeGarageOnboarding } from "$lib/utils/garageOnboarding";
import {
  armConnectionsInvite,
  resetConnectionsInvite,
} from "$lib/utils/connectionsInvite";
import {
  clearPreferredMode,
  loadPreferredMode,
  loadWizardPowersDone,
  loadWizardSpaceDone,
  loadWizardTrustDone,
  markWizardPowersDone,
  markWizardSpaceDone,
  markWizardTourDone,
  markWizardTrustDone,
  resetWizardRelationshipFlags,
  savePreferredMode,
  type PreferredMode,
} from "$lib/utils/preferredMode";
import {
  clearOnboardingIdentity,
  loadPrincipalName,
} from "$lib/utils/onboardingIdentity";
import {
  parseIdentityTeachInput,
  withIdentityUserId,
} from "$lib/utils/identityTeach";
import type { WizardBootstrap, WizardMode, WizardScreen } from "$lib/types/wizard";

/** FE relationship phases layered on top of Rust wizard screens. */
export type WizardUiPhase =
  | "arrive"
  | "space"
  | "mode"
  | "brain"
  | "extras"
  | "phone"
  | "ready";

class WizardStore {
  visible = $state(false);
  loading = $state(true);
  busy = $state(false);
  mode = $state<WizardMode>("none");
  screen = $state<WizardScreen>("screen1");
  existingProvider = $state<string | null>(null);
  existingModel = $state<string | null>(null);
  error = $state<string | null>(null);
  /** Desktop relationship flow; mobile stays on Rust screens. */
  uiPhase = $state<WizardUiPhase>("arrive");
  preferredMode = $state<PreferredMode | null>(loadPreferredMode());
  /** Engine warms in the background — never surface status in the wizard. */
  private engineWarmStarted = false;

  async bootstrap() {
    this.loading = true;
    this.error = null;
    try {
      this.applyBootstrap(await bootstrapWizard());
      this.syncUiPhaseFromState();
      this.warmEngineSilently();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      this.visible = false;
    } finally {
      this.loading = false;
    }
  }

  async beginRerun() {
    this.busy = true;
    this.error = null;
    try {
      resetWizardRelationshipFlags();
      clearPreferredMode();
      clearOnboardingIdentity();
      resetConnectionsInvite();
      this.preferredMode = null;
      this.engineWarmStarted = false;
      this.applyBootstrap(await beginWizardRerun());
      this.syncUiPhaseFromState();
      this.warmEngineSilently();
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.busy = false;
    }
  }

  async continue(screen1Model?: string | null) {
    await this.advance({
      action: "continue",
      screen1Model: screen1Model ?? undefined,
    });
  }

  async skipCurrent() {
    await this.advance({ action: "skip" });
  }

  async back() {
    if (!isTauriMobilePlatform()) {
      if (this.uiPhase === "space") {
        this.uiPhase = "arrive";
        return;
      }
      if (this.uiPhase === "mode") {
        this.uiPhase = "space";
        return;
      }
      if (this.uiPhase === "brain") {
        this.uiPhase = "mode";
        return;
      }
      if (this.uiPhase === "ready") {
        this.uiPhase = this.preferredMode === "workspace-ai" ? "brain" : "mode";
        return;
      }
    }
    await this.advance({ action: "back" });
  }

  async finish() {
    this.busy = true;
    this.error = null;
    try {
      this.applyBootstrap(await completeWizard());
      completeGarageOnboarding();
      // Tour was cut from the critical path — never block on it.
      markWizardTourDone();
      this.seedPrincipalNameSilently();
      if (isTauriMobilePlatform()) {
        layout.setMobileTab("chat");
        saveLastSurface("chat");
      } else {
        // MCP / messaging packages only make sense after the +brain path.
        if (this.preferredMode === "workspace-ai") {
          armConnectionsInvite();
        }
        layout.navigateDesktop("library");
        saveLastSurface("library");
      }
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.busy = false;
    }
  }

  async applyScreen1Setup(request: WizardApplyScreen1Request) {
    this.busy = true;
    this.error = null;
    try {
      const result = await applyWizardScreen1(request);
      if (!result.coreReady) {
        this.error = result.coreMessage;
        return result;
      }
      await this.continue(request.path);
      if (!isTauriMobilePlatform()) {
        this.markPathComplete();
        this.uiPhase = "ready";
      }
      return result;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.busy = false;
    }
  }

  completeArrive() {
    markWizardTrustDone();
    this.uiPhase = "space";
  }

  completeSpace() {
    markWizardSpaceDone();
    markWizardTrustDone();
    this.uiPhase = "mode";
  }

  /** @deprecated */
  completeTrust() {
    this.completeArrive();
  }

  /** @deprecated */
  completePersonalize() {
    this.completeSpace();
  }

  async choosePreferredMode(mode: PreferredMode) {
    savePreferredMode(mode);
    this.preferredMode = mode;
    markWizardTrustDone();

    if (mode === "workspace-ai") {
      this.uiPhase = "brain";
      return;
    }

    if (this.screen === "screen1") {
      await this.skipCurrent();
    }
    this.markPathComplete();
    this.uiPhase = "ready";
  }

  /** Brain skip: downgrade to workspace-only landing (no shame). */
  async skipBrain() {
    savePreferredMode("workspace");
    this.preferredMode = "workspace";
    await this.skipCurrent();
    this.markPathComplete();
    this.uiPhase = "ready";
  }

  continueExtras() {
    this.markPathComplete();
    this.uiPhase = "ready";
  }

  skipExtras() {
    this.markPathComplete();
    this.uiPhase = "ready";
  }

  continuePowers() {
    this.continueExtras();
  }

  private markPathComplete() {
    markWizardPowersDone();
    markWizardTourDone();
  }

  private warmEngineSilently() {
    if (this.engineWarmStarted) return;
    if (!this.visible) return;
    if (!isTauri() || isTauriMobilePlatform()) return;
    this.engineWarmStarted = true;
    void startEngine({ privateBrain: false }).catch(() => {
      /* invisible — brain screen / apply path will retry if needed */
    });
  }

  private seedPrincipalNameSilently() {
    const name = loadPrincipalName();
    if (!name || !isTauri()) return;
    void import("$lib/daemon")
      .then(({ rememberIdentityFact }) =>
        rememberIdentityFact(
          withIdentityUserId(parseIdentityTeachInput(`call me ${name}`)),
        ),
      )
      .catch(() => {
        /* optional — space still opens */
      });
  }

  private async advance(request: Parameters<typeof advanceWizard>[0]) {
    this.busy = true;
    this.error = null;
    try {
      const result = await advanceWizard(request);
      this.applyBootstrap(result);
      if (!isTauriMobilePlatform()) {
        this.syncUiPhaseAfterAdvance(request.action);
      }
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.busy = false;
    }
  }

  private applyBootstrap(result: WizardBootstrap) {
    this.visible = result.visible;
    this.mode = result.mode;
    this.screen = result.screen;
    this.existingProvider = result.existingProvider ?? null;
    this.existingModel = result.existingModel ?? null;
  }

  private syncUiPhaseFromState() {
    if (!this.visible) return;
    if (isTauriMobilePlatform()) {
      this.uiPhase =
        this.screen === "completion"
          ? "ready"
          : this.screen === "screen3"
            ? "phone"
            : "brain";
      return;
    }

    if (this.screen === "migration") {
      this.uiPhase = "ready";
      return;
    }

    if (!loadWizardTrustDone()) {
      this.uiPhase = "arrive";
      return;
    }

    if (!loadWizardSpaceDone()) {
      this.uiPhase = "space";
      return;
    }

    this.preferredMode = loadPreferredMode();
    if (!this.preferredMode) {
      this.uiPhase = "mode";
      return;
    }

    if (this.preferredMode === "workspace-ai" && this.screen === "screen1") {
      this.uiPhase = "brain";
      return;
    }

    if (this.screen === "screen3") {
      this.uiPhase = "phone";
      return;
    }

    // Packages are post-entry — never block Ready on extras.
    if (!loadWizardPowersDone()) {
      markWizardPowersDone();
      markWizardTourDone();
    }
    this.uiPhase = "ready";
  }

  private syncUiPhaseAfterAdvance(action: "continue" | "skip" | "back") {
    if (this.screen === "screen3") {
      this.uiPhase = "phone";
      return;
    }
    if (this.screen === "completion") {
      if (this.uiPhase === "phone") {
        this.uiPhase = "ready";
        return;
      }
      // Brain continue/skip lands on Ready (skipBrain handles downgrade separately).
      if (this.uiPhase === "brain" || action === "skip" || action === "continue") {
        this.markPathComplete();
        this.uiPhase = "ready";
      }
    }
  }
}

export const wizard = new WizardStore();
