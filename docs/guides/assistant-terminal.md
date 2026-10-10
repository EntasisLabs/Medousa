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
