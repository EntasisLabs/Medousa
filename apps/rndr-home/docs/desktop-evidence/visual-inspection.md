# Final synthetic GPU visual inspection

Origin: `--export-desktop`, **offline fixture GPU rendering**, not a native window
capture. No real account, credential, daemon or provider connection was involved.
The following final PNGs were opened with the image viewer and inspected as pixels,
not inferred only from successful export receipts. All carry the persistent
FIXTURE title and SYNTHETIC GPU RENDER / NO NETWORK footer.

| Final image | SHA-256 | Pixel inspection |
|---|---|---|
| `renders/dark-wide-runtime.png` | `e9d65471b831247d15573ee324b2cd170f310b46a35c2f5fe06b582ce345a72b` | Shared rail, grouped settings sidebar, selected Runtime Controls, links and honest offline snapshot; body and sidebar separated; sidebar scrollbar supports lower sections |
| `renders/dark-narrow-basement.png` | `0a44f8eea2fc95a6201064550b90d47d6d2a2ca51c5142fdee06d3c16dd55089` | Connection heading and 9-of-9 pager fit; unsupported connection/file operations described; read state remains visibly offline; long content scrolls instead of horizontal overflow |
| `renders/light-wide-preferences.png` | `550aa88f88046b16fb43826c4878d309850ade089fe009904730eabdb34ce8ad` | Light palette, distinct local appearance controls, no pretend persisted preference state; settings sidebar reaches host group via scrolling |
| `renders/light-narrow-runtime-detail.png` | `e7159ea536622f79b212082ba951ffa2b59f6fbf6c8ec65be6390028fc964194` | Narrow title no longer collides with the secondary header copy; local filter, detail/Back and explicit fixture availability/account labels fit; no login or execution offered |
| `renders/high-contrast-wide-mcp-disconnected.png` | `45d694de4ac37b99c6c931cb907527ced1fb9b168fd60d8a83defd23f6ed5ead` | High-contrast surfaces and labels, disconnected MCP detail remains No; settings/admin navigation is independent of chat |
| `renders/high-contrast-narrow-runtime-permission.png` | `95ff5ca21157376b9b9b4c40a5b326d186cb3930488ef5686b61099c73cf174f` | Permission denied is clearly distinguished from an empty list or connected state; directs authorization to existing app, no pretend approval/secret control |

Earlier iterations revealed three app-level presentation defects, fixed before
these final exports: fixture labeling was lost during frame synchronization;
wide settings needed their own scrollport for lower sections; narrow title and
secondary header copy collided. Persistent synthetic labeling, settings scroll
translation/pager tests, and narrow subtitle hiding now cover them.

**Residual visual/native gaps:** the narrow text rail is intentionally dense
(8-point labels; long names can be visually compact). It is a preview substitute
for production icon/tooltip navigation, not pixel-perfect shell or accessibility
parity. Final physical hit-target, scaling, screen-reader, keyboard/IME and native
window/compositor checks remain user-owned. The grouped sidebar is scrollable;
Connection is not expected to be visible simultaneously with every earlier section.
No image proves live connectivity, secure entry, permission changes or native IME
safety. Other exported cases were generated and bounded/layout-tested, not each
individually claimed to have received manual pixel inspection.

## Raw-log whitespace caveat

The implementation/source commit passed `git diff --check`. A full staged-evidence
check reports only Cargo's extra blank line at EOF in `test-default.log` and
`test-observe.log`. These fresh raw logs are kept exactly as captured; the
implementation tests/lint/build/render/docs checks all passed. See
`whitespace-check.log` for the exact diagnostic. No preserved earlier evidence was
modified or normalized. This whitespace-only evidence caveat is not presented as
a passing full-candidate whitespace check.
