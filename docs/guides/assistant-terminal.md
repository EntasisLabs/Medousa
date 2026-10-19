# Assistant terminal

**Audience:** Medousa users who enable shell tools for their assistant.

Enable agent shell tools in **Settings → Shell**. The assistant uses the same
shell command arguments and workshop paths as before. On macOS and Linux,
commands now run in a native terminal directly, without a Grapheme workflow.
The sandbox, network setting, allowed binaries, writable roots, and output
limit still apply.

A command that keeps running returns its terminal ID and initial output. The
assistant can poll that terminal, send input to a prompt, or interrupt it using
the existing shell session tools. Output and errors share the terminal stream;
the exit code is reported once the command finishes. The optional timeout is
an execution deadline, rather than the time spent collecting one response.

The terminal belongs to the exact turn that started it. Finishing or cancelling
that turn stops its commands. Another chat or later turn cannot take over the
terminal. The assistant may own up to eight terminals concurrently.

Windows retains the existing AppContainer/Job execution backend. Bound container
work environments keep their existing execution path. Persistent human terminal
tabs and Forge-bound Coder terminals continue to use the workshop session host.

Opening or polling a Coder terminal uses the daemon's in-memory connection
credential. It does not unlock Keychain to retrieve a local CLI token.
Local app and pairing credentials use private files; API keys and OAuth tokens
keep their Keychain storage. On upgrade, local connection tokens move to files
automatically at daemon startup, without unlocking Keychain. Existing remote
pairing or database credentials may need one import of their old Keychain
item. Home's credential status indicators use stored presence flags without
reading the Keychain values. If a Coder assignment expires, the agent can still report
the blocker, ask for input, checkpoint, or finish its turn.
