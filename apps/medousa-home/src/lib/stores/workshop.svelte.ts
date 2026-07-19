import {
  compileGraphemeSource,
  getGraphemeAllowlist,
  getGraphemeLifecycle,
  getGraphemeModule,
  getGraphemeScript,
  listGraphemeModules,
  listGraphemeScripts,
  loadGraphemeModule,
  runGraphemeSource,
  saveGraphemeScript,
  updateGraphemeAllowlist,
} from "$lib/daemon";
import type {
  GraphemeCompileResponse,
  GraphemeLifecycleEvent,
  GraphemeModuleDetailResponse,
  GraphemeModuleLoadResponse,
  GraphemeModuleSummary,
  GraphemeRunResponse,
  GraphemeScriptEntry,
} from "$lib/types/grapheme";

export class WorkshopStore {
  modules = $state<GraphemeModuleSummary[]>([]);
  scripts = $state<GraphemeScriptEntry[]>([]);
  allowlist = $state<string[]>([]);
  allowlistEnforce = $state(false);
  allowlistBusy = $state(false);
  allowlistError = $state<string | null>(null);
  moduleDetail = $state<GraphemeModuleDetailResponse | null>(null);
  moduleDetailId = $state<string | null>(null);
  moduleDetailLoading = $state(false);
  moduleDetailError = $state<string | null>(null);
  lifecycleEvents = $state<GraphemeLifecycleEvent[]>([]);
  lifecycleLoading = $state(false);
  lifecycleError = $state<string | null>(null);
  moduleLoadResult = $state<GraphemeModuleLoadResponse | null>(null);
  moduleLoadBusy = $state(false);
  moduleLoadError = $state<string | null>(null);
  editorScriptId = $state<string | null>(null);
  editorName = $state("");
  editorBody = $state("");
  editorTags = $state("");
  editorIntent = $state("");
  editorBusy = $state(false);
  editorError = $state<string | null>(null);
  editorSaved = $state<GraphemeScriptEntry | null>(null);
  compileResult = $state<GraphemeCompileResponse | null>(null);
  compileBusy = $state(false);
  compileError = $state<string | null>(null);
  runResult = $state<GraphemeRunResponse | null>(null);
  runBusy = $state(false);
  runError = $state<string | null>(null);
  error = $state<string | null>(null);
  loading = $state(false);

  async refreshModulesAndScripts() {
    this.loading = true;
    this.error = null;
    try {
      const [modules, scripts, allowlist] = await Promise.all([
        listGraphemeModules(),
        listGraphemeScripts({ limit: 100 }),
        getGraphemeAllowlist(),
      ]);
      this.modules = modules.modules;
      this.scripts = scripts.scripts;
      this.allowlist = allowlist.allowed_modules;
      this.allowlistEnforce = allowlist.enforce;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  async loadAllowlist() {
    this.allowlistBusy = true;
    this.allowlistError = null;
    try {
      const response = await getGraphemeAllowlist();
      this.allowlist = response.allowed_modules;
      this.allowlistEnforce = response.enforce;
    } catch (err) {
      this.allowlistError = err instanceof Error ? err.message : String(err);
    } finally {
      this.allowlistBusy = false;
    }
  }

  async saveAllowlist(modules: string[]) {
    this.allowlistBusy = true;
    this.allowlistError = null;
    try {
      const response = await updateGraphemeAllowlist(modules);
      this.allowlist = response.allowed_modules;
      this.allowlistEnforce = response.enforce;
    } catch (err) {
      this.allowlistError = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.allowlistBusy = false;
    }
  }

  isModuleAllowed(moduleId: string): boolean {
    if (!this.allowlistEnforce) return true;
    return this.allowlist.some(
      (entry) => entry.toLowerCase() === moduleId.toLowerCase(),
    );
  }

  toggleAllowlistModule(moduleId: string, enabled: boolean) {
    const normalized = moduleId.toLowerCase();
    const next = this.allowlist.filter(
      (entry) => entry.toLowerCase() !== normalized,
    );
    if (enabled) {
      next.push(moduleId);
    }
    void this.saveAllowlist(next);
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
    this.moduleLoadResult = null;
    this.moduleLoadError = null;
    try {
      this.moduleDetail = await getGraphemeModule(moduleId);
      await this.refreshLifecycle();
    } catch (err) {
      this.moduleDetail = null;
      this.moduleDetailError =
        err instanceof Error ? err.message : String(err);
    } finally {
      this.moduleDetailLoading = false;
    }
  }

  async refreshLifecycle() {
    this.lifecycleLoading = true;
    this.lifecycleError = null;
    try {
      const response = await getGraphemeLifecycle();
      this.lifecycleEvents = response.events;
    } catch (err) {
      this.lifecycleError = err instanceof Error ? err.message : String(err);
    } finally {
      this.lifecycleLoading = false;
    }
  }

  async loadWasmModule(moduleId: string, wasmPath: string, version?: string) {
    this.moduleLoadBusy = true;
    this.moduleLoadError = null;
    this.moduleLoadResult = null;
    try {
      this.moduleLoadResult = await loadGraphemeModule({
        module_id: moduleId,
        wasm_path: wasmPath,
        version: version ?? null,
      });
      await this.refreshLifecycle();
    } catch (err) {
      this.moduleLoadError = err instanceof Error ? err.message : String(err);
    } finally {
      this.moduleLoadBusy = false;
    }
  }

  clearModuleDetail() {
    this.moduleDetailId = null;
    this.moduleDetail = null;
    this.moduleDetailError = null;
    this.runResult = null;
    this.runError = null;
    this.moduleLoadResult = null;
    this.moduleLoadError = null;
    this.lifecycleEvents = [];
    this.lifecycleError = null;
  }

  resetEditor() {
    this.editorScriptId = null;
    this.editorName = "";
    this.editorBody = "";
    this.editorTags = "";
    this.editorIntent = "";
    this.editorError = null;
    this.editorSaved = null;
    this.compileResult = null;
    this.compileError = null;
  }

  async openScriptInEditor(scriptId: string) {
    this.editorBusy = true;
    this.editorError = null;
    try {
      const detail = await getGraphemeScript(scriptId);
      this.editorScriptId = detail.script.id;
      this.editorName = detail.script.name;
      this.editorBody = detail.body_preview;
      this.editorTags = detail.script.tags.join(", ");
      this.editorIntent = detail.script.intent ?? "";
      this.editorSaved = null;
      this.compileResult = null;
      this.compileError = null;
    } catch (err) {
      this.editorError = err instanceof Error ? err.message : String(err);
    } finally {
      this.editorBusy = false;
    }
  }

  async saveEditorScript() {
    this.editorBusy = true;
    this.editorError = null;
    this.editorSaved = null;
    try {
      const tags = this.editorTags
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean);
      const response = await saveGraphemeScript({
        id: this.editorScriptId,
        name: this.editorName.trim(),
        body: this.editorBody,
        tags,
        intent: this.editorIntent.trim() || null,
        modules: [],
      });
      this.editorScriptId = response.script.id;
      this.editorSaved = response.script;
      await this.refreshModulesAndScripts();
    } catch (err) {
      this.editorError = err instanceof Error ? err.message : String(err);
    } finally {
      this.editorBusy = false;
    }
  }

  /** Hard-delete a saved script and close any open editor / LME tabs for it. */
  async deleteScript(scriptId: string) {
    const { deleteScriptById } = await import("$lib/grapheme/scriptWorkbenchActions");
    await deleteScriptById(scriptId);
  }

  async compileEditorSource(mode: "check" | "aot" = "check") {
    this.compileBusy = true;
    this.compileError = null;
    this.compileResult = null;
    try {
      this.compileResult = await compileGraphemeSource(this.editorBody, mode);
    } catch (err) {
      this.compileError = err instanceof Error ? err.message : String(err);
    } finally {
      this.compileBusy = false;
    }
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
