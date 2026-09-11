# Draw in Medousa

Medousa drawings are vault-native Markdown. They can live inside an ordinary note or open as a full drawing note, and they sync through the workshop like any other note.

## Draw inside a note

Open a note in **Live**, type `/draw`, and choose **Drawing**. The same drawing
surface is also used by full drawing notes.

- Choose **Pen**, **Pencil**, **Marker**, or **Highlighter**, then select a color
  and size. Pen-capable devices use the stylus pressure supplied by the device;
  mouse and touch input use a stable speed-sensitive fallback.
- **Erase → Partial** removes only the ink under the eraser path. **Stroke**
  removes each complete stroke the path touches.
- **Select** and draw a lasso around or across ink. Drag selected ink to move
  it, or use **Duplicate** and **Delete**.
- **Hand** pans with a mouse or stylus. One-finger touch pans by default and two
  fingers pan or pinch-zoom. Turn on **Finger draws** when you intentionally
  want one-finger ink.
- Use the zoom percentage to reset the view, or the adjacent minus and plus
  buttons to zoom. **Undo**, **Redo**, and **Clear** apply to the active drawing.

Apple Pencil and Android active styluses use Pointer Events, including pressure,
tilt, and coalesced samples when the device WebView supplies them. While a pen
gesture is active, incidental touch input is ignored to reduce palm marks.

In Preview, the same block renders as a clean, read-only canvas. In Build, it remains a normal fenced Markdown block:

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
