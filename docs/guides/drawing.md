# Draw in Medousa

Medousa drawings are vault-native Markdown. They can live inside an ordinary note or open as a full drawing note, and they sync through the workshop like any other note.

## Draw inside a note

Open a note in **Live**, type `/draw`, and choose **Drawing**. Use the pen, color, and width controls; switch to **Eraser** to remove strokes. **Undo**, **Redo**, and **Clear** apply to the active drawing.

In Preview, the same block renders as a clean, read-only canvas. In Build, it remains a normal fenced Markdown block:

````markdown
```draw
version: 1
encoding: base64url
payload:
  ...
```
````

The payload is a versioned vector scene rather than a screenshot. Keep the complete fence together; Medousa updates it when a stroke finishes.

## Make a full drawing note

Choose **New note → Drawing**. A `kind: draw` note opens directly in the full drawing surface, like a ledger opens in its table surface. The underlying file still contains frontmatter, a title, and the same `draw` fence, so normal vault sync, versions, conflicts, Markdown export, PDF, and Word export continue to work.

## Storage and portability

- The workshop daemon remains the vault authority; the app saves the whole Markdown note through the existing note API.
- Base64url keeps arbitrary scene JSON from being mistaken for tags or wikilinks by Markdown tools.
- Version fields leave room for future shapes, text, pressure, and scene migrations without changing the note model.
- Markdown export preserves editable drawing data. Rendered exports freeze the visible canvas.
