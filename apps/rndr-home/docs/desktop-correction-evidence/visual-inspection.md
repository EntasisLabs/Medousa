# Direct actual synthetic pixel inspection

The first individual image request exposed artifact metadata without inspectable
pixels. Later runtime-attached image observations recovered actual pixels. The
final direct `cognition_image_view` contact-sheet observation was visually
inspected, not inferred from metadata, filenames, hashes or OCR.

Contact sheet: `minimum-height-contact-sheet.png`, 1600x1950, SHA-256
`b7a8beffd6a40839b5344b50667f8f7f85952b11728d43b9030a8c7dabb11ae2`.
Direct tool artifact: `art:ses_peer:cognition_image_view:output:b7a8beffd6a4`.
Rows: dark, light, high contrast. Columns: 1120x650 wide, 480x650 narrow.
`make-contact-sheet.swift` tiles the six unchanged source PNGs with no overlays.

| Actual sample | Inspected pixel findings |
|---|---|
| dark-wide-650-preferences | Appearance buttons and copy fit; sidebar grouping visible with lower sections below fold; full one-line footer including PROOF |
| dark-narrow-650-preferences | Previous/Next buttons and section hint fit; all three appearance controls and explanatory copy visible; complete two-line INPUT PROOF footer |
| light-wide-650-accounts | Settings sidebar/Accounts marker, dataset header, Refresh and offline state visible without overlap; footer complete |
| light-narrow-650-basement | Pager and focused filter visible; Connection copy extends into scrollable body, with visible scrollbar and lower health content below fold; footer complete |
| high-contrast-wide-650-mcp | Black/white theme, MCP heading and offline read-only state visible; sidebar below-fold content requires scrolling; footer complete |
| high-contrast-narrow-650-preferences | Pager, appearance labels, narrow rail words and explanatory copy visible; footer INPUT PROOF on second line, not clipped |

Across these six: complete readable Schedules/Settings rail text; You lies below
the initial 650px rail fold. No evident glyph/control overlap in the sampled
views. The wide settings column intentionally scrolls; do not mistake a viewport
boundary for missing destinations. Existing synthetic tests reach Connection
via sidebar scrolling and every narrow pager step at 650px. This is synthetic
layout/pixel evidence, not native chrome/pane/tab fidelity or physical UI proof.

Not claimed: exhaustive inspection of all 216 PNGs, accessibility contrast ratios,
physical focus/keyboard/IME/AT behavior, live service connectivity, device scaling,
or absence of clipping in uninspected long-list/detail states. No framework or
input gate was changed. No OCR was used as visual signoff.
