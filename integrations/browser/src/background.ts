import { savePendingContext } from "./storage.js";
import type { BrowserPageSnapshot } from "./types.js";

const SELECTION_MENU_ID = "medousa-ask-selection";
const PAGE_MENU_ID = "medousa-ask-page";

async function openPanel(windowId: number | undefined): Promise<void> {
  if (windowId === undefined) return;
  try {
    await chrome.sidePanel.open({ windowId });
  } catch {
    // The side panel can only be opened from a browser user gesture. The action
    // and context-menu handlers satisfy that requirement; unsupported hosts can
    // still open it from the toolbar normally.
  }
}

async function installMenus(): Promise<void> {
  await chrome.contextMenus.removeAll();
  chrome.contextMenus.create({
    id: SELECTION_MENU_ID,
    title: "Ask Medousa about this selection",
    contexts: ["selection"],
  });
  chrome.contextMenus.create({
    id: PAGE_MENU_ID,
    title: "Ask Medousa about this page",
    contexts: ["page"],
  });
}

async function configureSidePanel(): Promise<void> {
  try {
    // Route the toolbar click through onClicked so Chrome grants activeTab to
    // the extension before the side panel asks the page for DOM content.
    await chrome.sidePanel.setPanelBehavior({ openPanelOnActionClick: false });
  } catch {
    // Firefox and older Chromium builds can omit this convenience API.
  }
}

async function initialize(): Promise<void> {
  await Promise.all([installMenus(), configureSidePanel()]);
}

function snapshotFromTab(tab: chrome.tabs.Tab | undefined, selection = ""): BrowserPageSnapshot {
  return {
    tabId: tab?.id,
    windowId: tab?.windowId,
    title: tab?.title?.trim() || "Current page",
    url: tab?.url ?? "",
    selection: selection.trim(),
    text: "",
  };
}

chrome.runtime.onInstalled.addListener(() => void initialize());
chrome.runtime.onStartup.addListener(() => void initialize());
chrome.action.onClicked.addListener((tab) => void openPanel(tab.windowId));

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId !== SELECTION_MENU_ID && info.menuItemId !== PAGE_MENU_ID) return;
  const pending = {
    snapshot: snapshotFromTab(tab, info.menuItemId === SELECTION_MENU_ID ? info.selectionText ?? "" : ""),
    prompt: info.menuItemId === SELECTION_MENU_ID ? "Explain this selection." : "Summarize this page.",
    createdAt: Date.now(),
  };
  void savePendingContext(pending).then(() => openPanel(tab?.windowId));
});

chrome.commands.onCommand.addListener((command) => {
  if (command !== "open-medousa") return;
  void chrome.tabs.query({ active: true, currentWindow: true }).then(([tab]) => openPanel(tab?.windowId));
});

void initialize();
