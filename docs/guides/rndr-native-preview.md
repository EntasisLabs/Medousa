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

Attachments, voice, stickies, approval/secret fulfillment, remote workshops,
Behavior editing, full tabs/chat browsing, and other studio surfaces are not
ported. Use the existing app for those functions. The preview never provides
an enabled pretend approval/credential control. See the app README for tested
behavior, framework reproductions, and [verification evidence](../../apps/rndr-home/docs/VERIFICATION.md).
