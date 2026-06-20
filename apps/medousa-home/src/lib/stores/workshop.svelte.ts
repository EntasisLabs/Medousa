import {
  getGraphemeModule,
  listGraphemeModules,
  listGraphemeScripts,
  runGraphemeSource,
} from "$lib/daemon";
import type {
  GraphemeModuleDetailResponse,
  GraphemeModuleSummary,
  GraphemeRunResponse,
  GraphemeScriptEntry,
} from "$lib/types/grapheme";

export class WorkshopStore {
  modules = $state<GraphemeModuleSummary[]>([]);
  scripts = $state<GraphemeScriptEntry[]>([]);
  moduleDetail = $state<GraphemeModuleDetailResponse | null>(null);
  moduleDetailId = $state<string | null>(null);
  moduleDetailLoading = $state(false);
  moduleDetailError = $state<string | null>(null);
  runResult = $state<GraphemeRunResponse | null>(null);
  runBusy = $state(false);
  runError = $state<string | null>(null);
  error = $state<string | null>(null);
  loading = $state(false);

  async refreshModulesAndScripts() {
    this.loading = true;
    this.error = null;
    try {
      const [modules, scripts] = await Promise.all([
        listGraphemeModules(),
        listGraphemeScripts({ limit: 100 }),
      ]);
      this.modules = modules.modules;
      this.scripts = scripts.scripts;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  async loadModuleDetail(moduleId: string) {
    if (this.moduleDetailId === moduleId && this.moduleDetail) {
      return;
    }
    this.moduleDetailId = moduleId;
    this.moduleDetailLoading = true;
    this.moduleDetailError = null;
    this.runResult = null;
    this.runError = null;
    try {
      this.moduleDetail = await getGraphemeModule(moduleId);
    } catch (err) {
      this.moduleDetail = null;
      this.moduleDetailError =
        err instanceof Error ? err.message : String(err);
    } finally {
      this.moduleDetailLoading = false;
    }
  }

  clearModuleDetail() {
    this.moduleDetailId = null;
    this.moduleDetail = null;
    this.moduleDetailError = null;
    this.runResult = null;
    this.runError = null;
  }

  async runScriptSource(source: string) {
    this.runBusy = true;
    this.runError = null;
    this.runResult = null;
    try {
      this.runResult = await runGraphemeSource(source);
    } catch (err) {
      this.runError = err instanceof Error ? err.message : String(err);
    } finally {
      this.runBusy = false;
    }
  }
}

export const workshop = new WorkshopStore();
