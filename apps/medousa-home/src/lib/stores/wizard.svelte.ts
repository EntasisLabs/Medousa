import {
  advanceWizard,
  beginWizardRerun,
  bootstrapWizard,
  completeWizard,
} from "$lib/utils/wizardApi";
import {
  applyWizardScreen1,
  type WizardApplyScreen1Request,
} from "$lib/utils/providersApi";
import type { WizardBootstrap, WizardMode, WizardScreen } from "$lib/types/wizard";

class WizardStore {
  visible = $state(false);
  loading = $state(true);
  busy = $state(false);
  mode = $state<WizardMode>("none");
  screen = $state<WizardScreen>("screen1");
  existingProvider = $state<string | null>(null);
  existingModel = $state<string | null>(null);
  error = $state<string | null>(null);

  async bootstrap() {
    this.loading = true;
    this.error = null;
    try {
      this.applyBootstrap(await bootstrapWizard());
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
      this.applyBootstrap(await beginWizardRerun());
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
    await this.advance({ action: "back" });
  }

  async finish() {
    this.busy = true;
    this.error = null;
    try {
      this.applyBootstrap(await completeWizard());
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
      }
      await this.continue(request.path);
      return result;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.busy = false;
    }
  }

  private async advance(request: Parameters<typeof advanceWizard>[0]) {
    this.busy = true;
    this.error = null;
    try {
      const result = await advanceWizard(request);
      this.applyBootstrap(result);
      if (result.visible && result.screen === "completion" && request.action === "continue") {
        // Screen 3 → completion is a visible step; completion → home happens via finish().
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
}

export const wizard = new WizardStore();
