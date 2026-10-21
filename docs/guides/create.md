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

## Work on the canvas

- **Select** (`V`): click and drag objects. Draw a freehand lasso in empty space to select several;
  Shift adds to the selection. Frames are selected by clicking their background,
  so a drag inside a frame can still select its contents. Drag the bottom-right
  handle to resize. Arrow keys nudge; Shift-arrow moves ten units. Delete or
  Backspace removes the selected objects and their children. Command/Control-D
  duplicates the selection with its children.
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
  do not add placeholder objects.
- **Shapes** (`R`): choose Rectangle, Ellipse, Line, or Arrow in the tool picker,
  then drag to draw. Shapes return to Select after placement. Arrows reuse
  editable ink, including its color and resize controls.
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
floating selection row provides Duplicate, Delete, and Details.

**Details** shows **Layers** and the selected object's properties.
Adjust position, size, rotation, opacity, color, ordering, visibility, and locks.
Image crops preserve the original bytes. Frames can have transparent backgrounds.
A locked frame also prevents direct manipulation of its children.

Edits save after each completed gesture or property change. You can keep drawing
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

When a result is ready, switch between **Original** and **Preview**. **Use this**
applies the proposal as an undoable edit; dismissing preserves the original. A
proposal from an earlier document revision cannot overwrite subsequent edits.
Start a new request against the updated selection instead.

Bitmap generation uses your existing **Image generation** model setting and
credentials. A generated image stays an image layer; Medousa does not present it
as recovered editable text or shapes.

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
