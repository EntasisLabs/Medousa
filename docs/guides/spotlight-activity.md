# Follow workshop activity in Spotlight

Open Spotlight with **⌘K** on macOS or **Ctrl+K** on Windows/Linux. Choose
**Home → Activity** to follow the connected workshop's recent and running work.

Activity includes background tasks, delegated workers and their tool runs,
live conversations loaded in Medousa, code projects with attempts and command
runs, scheduled runs, and recent note activity. Available history follows what
the connected daemon retains. Some sources may be unavailable on older daemons;
the other activity remains usable.

- Expand a row to see its children. Conversations and delegated turns use their
  recorded session and correlation identifiers. Code commands remain under their
  project; they are not assigned to an attempt unless that relationship is known.
- Choose **All**, **Running**, **Needs you**, or **Finished**. Filtering and search
  keep matching children's ancestors visible. Failed terminal runs are included
  in Finished as well as Needs you.
  All follows your existing setting for hiding old terminal tasks. Search,
  Finished, and Needs you can also inspect the older tasks still retained by the daemon.
- Type in the Spotlight search box to search Activity while this lens is selected.
  Select another lens to return to the normal command and object search.
- Select a row to preview its status, progress, and retained output. Execution
  output loads for the selected run. **Expand details** gives that preview more
  room. **Open chat**, **Open project**, **Open review**, and **Open note** go to
  the relevant existing surface.
  For tasks and workers, **Task details** opens the shared inspector over your
  current screen. Standalone tasks use **Open task**. Status-bar task picks,
  linked tasks, and task notifications open that same inspector without switching
  to Work. Close it or press Escape to return to what you were viewing.
- Use **↑/↓** to select, **←/→** to collapse or expand, **Enter** to open, and
  **Escape** to close Spotlight.

Activity follows the existing workspace stream and refreshes code and schedule
summaries every 15 seconds while open and visible. Closing the lens stops its
refreshes and releases its local summaries and preview output. Code project
metadata loads in pages across the catalog. A load-more control enriches
additional tasks without fetching every transcript at once. Recent projects preload command summaries;
expand an older project to load its command history. **Refresh activity** retries
available sources.

The existing Work view remains available during the transition. Activity can
preview tasks and runs directly without opening Work.

Background ask completion handling is available across desktop tabs and mobile
screens. Completions are queued when several tasks finish close together. The
completion dialog offers **View task**, journal and delivery actions, and
**Later**; opening task details defers other completion dialogs until you close
the inspector. Files and results belong to the connected workshop. Closing task
details releases its local output and preview state; late responses from a previous task or
workshop do not replace the current selection.
