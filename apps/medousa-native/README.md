# Medousa native desktop

Fresh RNDR application; no dependency on `apps/rndr-home`.

```sh
cd apps/medousa-native
cargo run
```

Uses the sibling RNDR checkout at `../../../../rndr`, shared
`medousa-app-core`, and shared desktop-host connection discovery. On launch it
attempts to attach to the configured local daemon using the existing local
credentials. It does **not** start, install, or restart a daemon. An unavailable
daemon is an ordinary connection error, not a separate preview application.

The shell contains a left conversation sidebar and native chat workspace.
Session search/paging and mutations, model/mode/depth controls, source-document
composition, typed history/stream rendering, attachments, runtime controls,
approvals, and session context use the existing daemon contracts. RNDR owns
editing, selection, controls, layout, Markdown, and platform rendering.

```sh
cargo test
```

The test suite uses fixture transports and headless RNDR frames. Passing it is
not evidence that every workflow has been manually exercised against a live
daemon or that arbitrary web/HTML content has native parity.
