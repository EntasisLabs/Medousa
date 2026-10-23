/** Optional editor host port; ordinary tabs do not register a native close hook. */
export const WINDOW_SAVE = Symbol("desktop-window-save");
export type WindowSaveRegistration = (save: () => Promise<boolean>) => () => void;
