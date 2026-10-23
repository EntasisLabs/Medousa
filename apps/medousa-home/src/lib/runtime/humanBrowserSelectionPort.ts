/** Composition hook: choosing a device tab must leave workshop presentation. */
let selectDevice = () => {};
export function setHumanBrowserSelectionPort(select: () => void) { selectDevice = select; }
export function selectDeviceBrowser() { selectDevice(); }
