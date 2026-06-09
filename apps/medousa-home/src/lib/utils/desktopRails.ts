const NAV_WIDTH = 52;
const MAIN_MIN_WIDTH = 280;
const ACTIVITY_MIN = 220;
const ACTIVITY_MAX = 520;
const ACTIVITY_STRIP = 28;
const INSPECTOR_MIN = 280;
const INSPECTOR_MAX = 560;

export interface DesktopRailsInput {
  viewportWidth: number;
  activityCollapsed: boolean;
  activityWidth: number;
  workInspectorOpen: boolean;
  workInspectorWidth: number;
}

export interface DesktopRailsLayout {
  /** Render the narrow strip instead of the full activity panel. */
  showActivityStrip: boolean;
  activityPaneWidth: number;
  activityPaneMax: number;
  inspectorPaneWidth: number;
  inspectorPaneMax: number;
}

/** Keeps nav + main + optional inspector + activity within the viewport without overlap. */
export function layoutDesktopRails(input: DesktopRailsInput): DesktopRailsLayout {
  let remaining = input.viewportWidth - NAV_WIDTH - MAIN_MIN_WIDTH;

  if (input.activityCollapsed) {
    remaining -= ACTIVITY_STRIP;
    const inspectorMax = input.workInspectorOpen
      ? Math.max(INSPECTOR_MIN, remaining)
      : INSPECTOR_MIN;
    const inspectorPaneWidth = input.workInspectorOpen
      ? clamp(input.workInspectorWidth, INSPECTOR_MIN, Math.min(INSPECTOR_MAX, inspectorMax))
      : 0;
    return {
      showActivityStrip: true,
      activityPaneWidth: ACTIVITY_STRIP,
      activityPaneMax: ACTIVITY_MAX,
      inspectorPaneWidth,
      inspectorPaneMax: inspectorMax,
    };
  }

  if (input.workInspectorOpen) {
    if (remaining < ACTIVITY_MIN + INSPECTOR_MIN) {
      remaining -= ACTIVITY_STRIP;
      const inspectorMax = Math.max(INSPECTOR_MIN, remaining);
      const inspectorPaneWidth = clamp(
        input.workInspectorWidth,
        INSPECTOR_MIN,
        Math.min(INSPECTOR_MAX, inspectorMax),
      );
      return {
        showActivityStrip: true,
        activityPaneWidth: ACTIVITY_STRIP,
        activityPaneMax: ACTIVITY_MAX,
        inspectorPaneWidth,
        inspectorPaneMax: inspectorMax,
      };
    }

    const inspectorMax = Math.max(INSPECTOR_MIN, remaining - ACTIVITY_MIN);
    const inspectorPaneWidth = clamp(
      input.workInspectorWidth,
      INSPECTOR_MIN,
      Math.min(INSPECTOR_MAX, inspectorMax),
    );
    remaining -= inspectorPaneWidth;
    const activityMax = Math.max(ACTIVITY_MIN, Math.min(ACTIVITY_MAX, remaining));
    const activityPaneWidth = clamp(input.activityWidth, ACTIVITY_MIN, activityMax);

    return {
      showActivityStrip: false,
      activityPaneWidth,
      activityPaneMax: activityMax,
      inspectorPaneWidth,
      inspectorPaneMax: inspectorMax,
    };
  }

  const activityMax = Math.max(ACTIVITY_MIN, Math.min(ACTIVITY_MAX, remaining));
  const activityPaneWidth = clamp(input.activityWidth, ACTIVITY_MIN, activityMax);

  return {
    showActivityStrip: activityPaneWidth <= ACTIVITY_STRIP,
    activityPaneWidth,
    activityPaneMax: activityMax,
    inspectorPaneWidth: 0,
    inspectorPaneMax: INSPECTOR_MAX,
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
