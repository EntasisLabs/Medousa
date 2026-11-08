# Try the native rndr chat preview

This developer preview is a **second** Medousa desktop app in
`apps/rndr-home`; the normal Tauri app and home-first install are unchanged.
It requires a local rndr checkout and an existing configured local Medousa
workshop/daemon. It does not install or launch the daemon or pair remote hosts.

From the repository root, preview the UI without connecting or sending anything:

```sh
cargo run --manifest-path apps/rndr-home/Cargo.toml -j 2 -- --offline
```

For real chat, follow the [app setup and launch instructions](../../apps/rndr-home/README.md).
Pass the existing workshop's daemon origin and data directory. Do not copy
credentials or put tokens in the command line. The app reads the existing
first-party local credential boundary; provider setup remains in the normal app.

Use **+** to create a chat or **Continue where we left off** to resume the most
recent one. Use `--session ID` for a specific older chat. Choose a provider/model
with the selector, write a message, and press **Send ↑** or Enter. Sending can
incur inference charges; launching/resuming does not send a prompt. Shift+Enter
adds a newline. Ambiguous batched/IME submissions are deferred visibly, preserving
the draft. **Stop** asks the daemon to cancel; **Reload history** reconciles a
failed/disconnected turn before another send is allowed.

The model selector now opens a searchable provider-grouped menu rather than
cycling. **Mode** uses the daemon's effective session mode and available-mode
registry. **Behavior → Response depth** offers Concise, Standard, and Deep for
the next turn in this window (not persisted shared defaults). The composer grows
with wrapped text, up to six visual lines, and the transcript uses separate user
bubbles and assistant Markdown with Copy controls. **Runtime** shows the native
runtime; external agent choices are visibly disabled, not silently substituted.
This app always sends native turns and does not import Svelte's local runtime
preferences or external-session bindings.

**Behavior → Window appearance** offers light/dark appearance and a high-contrast
toggle using rndr's semantic controls. These settings affect this window only;
they are not saved, do not follow the OS, and never submit a prompt or change
workshop defaults. The default is dark. Launch with `--light` and/or
`--high-contrast` to choose an explicit initial appearance, including with
`--offline` or `--export`. The composer and transcript share rndr's bounded
reading measure; theme changes retain the draft, focus, and mounted messages.
High contrast is a palette option, not a claim of full native accessibility
certification.

Attachments, voice, stickies, approval/secret fulfillment, remote workshops,
external runtimes, remaining Behavior controls, full tabs/chat browsing, and other
studio surfaces are not ported. Use the existing app for those functions. The preview never provides
an enabled pretend approval/credential control. See the app README for tested
behavior, framework reproductions, [interaction-parity findings](../../apps/rndr-home/docs/INTERACTION_PARITY.md), and [verification evidence](../../apps/rndr-home/docs/VERIFICATION.md).
