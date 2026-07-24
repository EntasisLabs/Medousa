/** Cross-component script workbench chrome (status bar → console). */

export const SCRIPT_WORKBENCH_OPEN_CONSOLE_EVENT =
  "medousa-script-workbench-open-console";

export function dispatchScriptWorkbenchOpenConsole() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(SCRIPT_WORKBENCH_OPEN_CONSOLE_EVENT));
}
