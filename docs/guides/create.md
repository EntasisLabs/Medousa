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
  its font proportionally.
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
Pen input takes priority over nearby palm touches, as it does in drawing notes.

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

For text, **Style** includes a font family, exact font size with smaller/larger
buttons, **Bold**, and **Italic**. These controls also work while typing, keeping
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
applies the proposal as an undoable edit; dismissing preserves the original. A
proposal from an earlier document revision cannot overwrite subsequent edits.
Start a new request against the updated selection instead.

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
