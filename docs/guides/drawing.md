# Draw in Medousa

Medousa drawings are vault-native Markdown. They can live inside an ordinary note or open as a full drawing note, and they sync through the workshop like any other note.

## Draw inside a note

Open a note in **Live**, type `/draw`, and choose **Drawing**. The same drawing
surface is also used by full drawing notes.

- On desktop, use the compact drawing bar to switch between **Draw**, **Erase**,
  **Select**, and **Move**. On iPhone and iPad, drawing controls live in the
  note's top bar instead: tap the current tool to choose another mode, then use
  the adjacent buttons for **Drawing options**, **Undo**, and **Redo**. This
  leaves the full height below the note header available to the canvas.
- Open **Drawing options** (the sliders button) for brushes, color, size, eraser
  mode, touch behavior, view controls, and document actions. On a phone these
  controls open as a bottom sheet instead of a scrolling toolbar.
- Choose **Pen**, **Pencil**, **Marker**, or **Highlighter** in the options sheet.
  Pen-capable devices use an expressive curve over the stylus pressure supplied
  by the device, making ordinary light-to-firm Apple Pencil input visibly change
  the stroke width. Mouse and touch input use a stable speed-sensitive fallback.
- **Erase → Partial** removes only the ink under the eraser path. **Stroke**
  removes each complete stroke the path touches.
- **Select** and draw a lasso around or across ink. Drag selected ink to move
  it, or use **Duplicate** and **Delete**.
- **Move** pans with a mouse or stylus. One-finger touch pans by default and two
  fingers pan or pinch-zoom. Turn on **Finger draws** when you intentionally
  want one-finger ink.
- Use **Fit drawing** to recover the view, or the adjacent minus and plus buttons
  to zoom. **Undo**, **Redo**, and **Clear drawing** apply to the active drawing.

The canvas is intentionally blank and edge-to-edge. It has no paper boundary or
grid, and it adapts its coordinate viewport to the available screen instead of
letterboxing a landscape canvas on portrait devices.

Apple Pencil and Android active styluses use Pointer Events, including pressure,
tilt, and coalesced samples when the device WebView supplies them. While a pen
gesture is active, incidental touch input is ignored to reduce palm marks.
Medousa also suppresses the WebView's long-press selection and callout gestures
inside the canvas so a pressure hold remains drawing input.

Ink and erasing render locally during the gesture. Medousa batches the drawing
back into the note after a short pause and holds vault autosave while a stylus
is active, keeping persistence work off the live input path without risking an
unfinished drawing when the surface closes.

In Preview, the same block renders as a clean, read-only canvas and the normal
note actions return to the mobile top bar. In Build, it remains a normal fenced
Markdown block:

````markdown
```draw
version: 2
encoding: base64url
payload:
  ...
```
````

The payload is a versioned vector scene rather than a screenshot. Keep the
complete fence together; Medousa updates it after drawing, erasing, or moving
ink. Version-1 drawings are migrated automatically when opened and remain
visually compatible.

## Make a full drawing note

Choose **New note → Drawing**. A `kind: draw` note opens directly in the full drawing surface, like a ledger opens in its table surface. The underlying file still contains frontmatter, a title, and the same `draw` fence, so normal vault sync, versions, conflicts, Markdown export, PDF, and Word export continue to work.

## Storage and portability

- The workshop daemon remains the vault authority; the app saves the whole Markdown note through the existing note API.
- Base64url keeps arbitrary scene JSON from being mistaken for tags or wikilinks by Markdown tools.
- Version 2 stores brush, input, pressure, timing, and tilt metadata while keeping
  the retained vector scene portable.
- Markdown export preserves editable drawing data. Rendered exports freeze the visible canvas.
