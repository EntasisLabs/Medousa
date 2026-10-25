# Create in Medousa

**Create** is a workspace for sketches, image compositions, diagrams, and simple
visual layouts. Projects belong to the connected workshop and keep their editable
canvas and images independently of conversations and notes.

## Start a project

Choose **Create** in the main navigation. Start with **Blank canvas**,
**Screenshot markup**, **Diagram**, **UI concept**, or **Image composition**.
Blank canvas starts with open space. The other starters include an editable
frame; image composition uses a square frame. Import
an image with the image button in the floating tool strip for screenshot markup
or a composition.

Projects appear under **Recent** in the rail. Open one to create or focus its
existing shell tab. Tabs use the same drag-to-split controls as other work. The
pin button beside a project's title adds it to **Pinned**. Click the title to
rename it. Returning to Create's browser does not open a random previous canvas.
On phones, Create is available in the destinations menu.

The rail's **+** menu offers the same project starters. Use its search button to
find a project; Escape closes search. The **…** menu sorts by recently edited or
name and includes Refresh. **Pinned** and **Recent** can be collapsed. Sorting,
section choices, and list position are remembered on this device per workshop.
Search shows matches even inside collapsed sections.

Hover a project and choose **…**, or right-click it, to rename, pin/unpin, or
delete it. Delete asks for confirmation; finish pending saves before deleting.
It removes the project's canvas and images and closes its open tabs. Arrow keys
move focus through the rail; Home/End jump to its ends, Enter opens a project,
F2 renames, and Delete/Backspace asks to delete. Left/right collapse or expand a
focused section. Rename and pin share the canvas's existing save and undo history.

## Work on the canvas

- **Select** (`V`): click and drag objects. Drag a box in empty space to select several;
  **Box** is the default; choose **Lasso** in **Style** for freehand selection.
  Shift adds to the selection. Frames are selected by clicking their background,
  so a drag inside a frame can still select its contents. Drag a corner or edge
  handle to resize; Shift preserves a single object's proportions. Groups and
  multiple selected objects share one box and resize proportionally, including
  their ink and text. Arrow keys nudge; Shift-arrow moves ten units. Delete or
  Backspace removes the selected objects and their children. Command/Control-D
  duplicates the selection with its children. Option/Alt-drag makes a copy while
  moving; Escape cancels the copy. Drag the round handle above a selection to
  rotate it around its center; Shift snaps to 15° steps. Groups and multiple
  objects rotate together, including their children. Escape cancels rotation.
- **Move** (`H`): drag to move the view. Holding Space or using the middle mouse
  button also pans. Scroll pans; Control/Command-scroll zooms around the pointer.
  **Fit** returns the complete scene to view.
- **Draw** (`P`): uses the same Pen, Pencil, Marker, and Highlighter presets as
  drawing notes. Choose the brush, color, and size in **Style**. Brush sizes keep
  the same apparent thickness as you zoom, within the document's supported range.
  Supported styluses supply pressure, tilt, and coalesced samples.
- **Erase** (`E`): **Partial ink** removes just the ink under the eraser; remaining
  pieces stay editable. **Whole object** removes complete strokes or other
  objects. The eraser cursor shows its radius. One gesture is one undoable edit;
  Escape cancels a gesture.
- **Text** (`T`): click to type directly on the canvas, or double-click existing
  text to edit it. Double-clicking empty space also starts text. Escape,
  Command/Control-Enter, or clicking outside finishes the edit. Empty new entries
  do not add placeholder objects. The editor is transparent and uses the same
  font, line spacing, and measured bounds as the saved text. Text grows and
  shrinks to fit its content, including manual line breaks. Corner handles scale
  its font proportionally. Drag the left/right side handles to wrap text to a
  box width instead; the height grows with its content. Hold Shift on a side
  handle to scale instead of wrapping.
- **Shapes** (`R`): choose Rectangle, Ellipse, Line, or Arrow in the tool picker,
  then drag to draw. Shapes return to Select after placement. Lines and arrows
  have two endpoint handles: drag either tip to change their direction or length,
  or drag the shaft to move the whole object. Shift snaps drawing and endpoint
  movement to 45° directions. Arrowheads stay sharp at every zoom level.
- **Frame** (`F`, or in Shapes): drag out another export area. New objects are placed in the
  selected frame or the frame underneath the pointer.
- **Import image**: choose, drop, or paste a PNG, JPEG, or WebP image up to 25 MiB. Its bytes are
  stored by the workshop, not left as a path to the client computer.

On touch screens, one finger moves the canvas by default and two fingers pan
and pinch to zoom. Turn on **Finger draws** in Style to draw with one finger.
Pen input takes priority over nearby palm touches, including while moving endpoints,
resizing, placing shapes, and rotating. Selection handles have larger invisible
hit areas, with larger touch targets on touch-capable devices. A second finger
cancels an in-progress touch transform and begins canvas navigation. **Multi** in
the tool strip adds or removes objects from the selection without a keyboard;
turn it off for ordinary selection. Finger drawing remains an explicit setting.

**Style** opens a compact palette. Choose a color swatch, an S/M/L/XL size, or
no fill, soft fill, or solid fill for shapes. These controls update selected
objects, or set the style for the next object when nothing is selected. The
floating selection row provides Group/Ungroup, Align, Arrange, Duplicate, Delete,
and Details when applicable. Color and size controls appear only for tools or
objects that use them.

For lines and arrows, **Style** offers thickness, solid or dashed strokes, and
an arrowhead at either or both ends. **Details** includes the same controls.
Line and arrow defaults are remembered separately. Existing tool-created arrows
get endpoint controls too; ordinary freehand strokes keep their drawing tools.

For rectangles, ellipses, frames, and images, **Style** and **Details** include
surface controls. Set shape fill and border independently, use a zero-width border for
no outline, and add a soft shadow with offset, blur, color, and strength. Rectangles,
frames, and images support rounded corners; drag the small inset circle near the
selection's top-left corner or enter an exact **Radius**. Frame contents and image
crops follow the rounded edge. These properties save with the document and share
its undo history. Canvas exports and AI visual context use the same styling;
exporting a frame keeps the exact artboard dimensions.

For text, **Style** includes font families with a sample preview and an unavailable-font
notice, a custom family field, weights from 100 to 900, and exact font size with
smaller/larger buttons. Use **Bold**, **Italic**, **Underline**, alignment, line
spacing, letter spacing, and paragraph spacing. Select words while editing to
change their emphasis, weight, underline, or color independently; size, family,
and layout controls apply to the whole text box. Command/Control-B, I, and U
apply emphasis while editing. Pasted text retains plain text, without importing HTML.
Choose **Auto width** for labels or **Wrap to width** for paragraphs. Side handles
change the wrap width; corner handles scale both font size and a fixed box.
These controls also work while typing, keeping
the draft and its styling in the same completed edit. The selection updates when
the font or content changes. **Details** offers the same text controls and a text
field for longer edits. Text defaults are remembered for new labels on this device.

Select objects in the same frame or group and use **Group** (Command/Control-G)
to keep them together. A group moves, duplicates, and resizes as one object.
Double-click to work inside it; Escape returns to the containing group or canvas.
Use **Ungroup** (Shift-Command/Control-G) to release its children. Set a group's
opacity to 100% before ungrouping so overlapping artwork keeps its appearance.

**Details**, at the canvas's top left, opens a floating panel with **Layers** and
the selected object's properties. **Style** floats on the right; both panels can
stay open without shifting or resizing the canvas. On narrow canvases, opening
one tucks the other away. Collapse **Properties** or **Layers** to keep the panel
compact; those section choices are remembered on this device for each workshop.
Adjust position, size, rotation, opacity, color, ordering, visibility, and locks.
Image crops preserve the original bytes. Frames can have transparent backgrounds.
A locked frame also prevents direct manipulation of its children.
Layers are nested inside their frames and groups. Use the chevron to collapse a
container. Double-click a layer name, or press F2, to rename it; renaming a text
layer does not change the text on the canvas. Shift-click selects a range;
Command/Control-click toggles individual layers. Arrow keys navigate the layer
list, and left/right collapse or expand containers.

Tool choices, individual brush sizes, eraser mode, color, and selection mode are remembered
on this device for each workshop. Returning to an open project restores its selection
and view.

Edits save quietly after each completed gesture or property change. You can keep drawing
while the workshop saves earlier gestures; a bounded queue retains those edits in
order. Tabs and splits share the optimistic canvas, document revision, and undo
history. Undo, export, image import, and AI requests wait for queued saves to finish. Use **Undo** and **Redo**, or
Command/Control-Z and Shift-Command/Control-Z. If saving fails, the failed edit and later queued edits stay
visible. **Retry** resends the exact failed request before saving later gestures. **Discard draft and reload** resolves
the local draft against the workshop's current version; it does not undo a change
that the workshop already committed.

## Grid, guides, and arranging objects

Open **View** in the shared **View | Style** control to choose a dot or line
grid and 8/16/32/64 px spacing. Opening View closes Style; opening Style closes View.
**Snap to grid** works independently of grid visibility for moving objects,
placing shapes/frames, resizing, and line endpoints. **Smart guides & snapping**
helps match nearby edges and centers while moving or duplicating objects. Hold
**Ctrl** during a drag to temporarily bypass snapping. Freehand ink is unaffected.
These settings are remembered per workshop on this device; the grid and guides
never appear in exported images or AI snapshots.

Select objects and open **Align** in the floating selection controls. Choose
**Selection** to line objects up with each other, or **Frame** to use the enclosing
frame. A single object aligns within its frame, including rotated frames and
nested groups. With three or more objects, **Space horizontally/vertically**
equalizes the gaps while leaving the outer objects fixed. Alignment and spacing
use normal save/undo. **View → Zoom to selection** focuses the current selection.

## Ask Medousa about a selection

Select objects or a frame, open **Ask Medousa** in the floating toolbar, and
describe the change. Closing the Ask panel keeps its draft and job; the toolbar
indicates an active request or a ready proposal. This first implementation uses **@Medousa** and the existing assistant
runtime. Other bot and external-agent targets are not connected to creative jobs
yet.

The request captures the selected objects and document revision. Work continues
on the workshop if you leave the tab. Returning to the project restores the job's
status. Use the stop button to cancel an executing request, or **Open conversation**
to inspect its full session.

Each request includes a rendered view of the selection, with nearby artwork
inside the crop, plus the original images used by the selected objects. This lets
Medousa see the composition and use those images as generation/editing references.
The capture excludes selection handles and toolbars and does not change the
canvas, save revision, or undo history. Select at most four distinct images per
request so the composition preview also fits in the assistant's image context.
Visual understanding depends on the configured assistant model's vision support.

When a result is ready, switch between **Original** and **Preview**. **Use this**
applies the proposal as an undoable edit; dismissing preserves the original.
You can keep working while generation runs. Preview and **Use this** combine the
result with compatible canvas edits, including unrelated objects and movement.
If both edits change the same field, or a removal would delete new work, applying
explains the conflict and keeps the generation saved. Undoing the conflicting
edit lets you try the same result again without regenerating.

Keep typing in Ask Medousa to continue the same conversation. After applying a
result, follow-ups capture the current saved canvas. To refine a suggestion
before applying it, choose **Refine preview**, then describe the next change.
The replacement preview includes the earlier suggestion and your refinement;
**Use this** applies the combined result in one undoable edit. An older preview
is superseded when its follow-up is admitted. **New conversation** starts a fresh
thread. The compact conversation history shows recent requests; **Open conversation**
opens the full runtime transcript. Finished older requests are archived so long
conversations can continue beyond 32 requests.

Bitmap generation uses your existing **Image generation** model setting and
credentials. A generated image stays an image layer; Medousa does not present it
as recovered editable text or shapes.
The current image-generation adapter uses an OpenAI API key. A ChatGPT-backed
assistant conversation can continue to use its existing connection, but this
build cannot generate images directly through that subscription connection.

If a creative request reports **Reconnect ChatGPT**, reconnect in
**Settings → Medousa Agent → Models**, then submit a new request. Failed generation
never applies changes to your canvas.

## Export and existing drawings

Choose **Export → PNG image** or **SVG document**. Selecting a frame exports that
frame; otherwise export includes the scene. Images are embedded in the export.
PNG respects transparent frame backgrounds. Exports are limited to 8,192 pixels
per side and 32 megapixels.

Canvas image previews are bounded to 24 images and 32 MiB of source image bytes
per view; closing the view releases its image URLs. Projects and unresolved drafts
are retained in a bounded shared session pool.

Existing [drawing notes](drawing.md) and chat drawings keep their supported
editing paths. Opening them does not silently migrate them or create a second
creative project. Notes-to-Create adoption and live embeds are separate follow-up
work.

## Move between Create and Code

Select one unlocked frame and choose **Build in Code** in the project header.
Choose an existing Code project and describe what to build. Save open Code files
first. Medousa receives a saved image of the frame and a compact editable blueprint,
then uses the existing Code conversation and repository tools to implement it.
The conversation opens so you can follow the work and review the source changes.

The frame remembers its project and conversation. Frames imported from the same page reuse that conversation when sent to the same project. **Update Code** sends a new
saved snapshot to that project; moving objects does not patch the running site.
Changes to the repo refresh its existing dev server through the project's normal
workflow. Canvas changes made after a handoff wait for the next Update Code.
If sending times out, use **Check conversation** before submitting again.

Choose **Import page** on Create's home screen or in the rail's **+** menu to
start from a running site. You can also choose **Import into Create** from the
browser's page menu. Pick an optional Code project and a running workshop browser,
or enter a dev-server URL to open it in a new managed workshop browser. A URL opens
on the workshop's machine; `localhost` refers to that machine. Opening a URL does
not copy another browser's sign-in state, unsaved inputs, or scroll position.

Choose **Full page · stacked frames** (the default) or **Current viewport**.
Full-page capture scrolls the page into view, waits briefly for visible images and
fonts, and restores the browser's position even if capture fails. It supports up
to 12 screenfuls and trims the final slice so the bottom is not duplicated.
Fixed/stuck navigation is included in the first editable slice; the captured
originals preserve what was visible at each position.

Editable frames stack vertically. The canvas opens focused on the first frame;
use the page arrows or frame selector in the header to move between them, or
**Fit canvas** to see the complete stack. **View original** opens the active frame's
read-only screenshot in a dismissible preview, including when selecting a child
inside that frame. Originals stay in workshop-owned assets and do not occupy
canvas space. Older imports keep their saved originals and undo history; the
editor presents their originals through the same preview. Text and simple surfaces become editable objects;
images, icons, and complex graphics remain cropped image layers. Font availability,
clipping, gradients, shadows, and complex CSS can require manual adjustment.
Capture requires a stable page and reports limits before you open the design.
Viewport mode captures one screenful; full-page mode captures the vertical page. Neither imports interactive behavior. Form
values are excluded from editable text; screenshot redaction follows the existing
browser rules. Normal save, undo, export, and Ask Medousa work on the imported copy.
Select its frame and use **Update Code** when ready to implement your redesign.
Both directions require updated app and daemon builds.

## Start from a conversation

Paste a screenshot into Chat or Spotlight and ask Medousa for a wireframe.
Medousa creates a frame scaffold and proposes editable text and shapes. Open the
returned Create link to preview the proposal and choose **Use this**; applying is
one undoable transaction. Proposals made from a conversation appear in the same
canvas controls as Create's own requests. Apply a chat proposal before refining
its preview from the canvas.

Create requests keep your wording as the user message. Selection IDs, captured
images, revision information, and job instructions travel separately through the
runtime. **Build in Code** and **Update Code** likewise pass an immutable design
reference that Medousa reads with tools, so later canvas changes cannot silently
alter an already submitted handoff.
