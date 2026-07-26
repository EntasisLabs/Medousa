# Writing scripts

**Advanced** — use this when you’re ready to write the body of a script, not just click Automations chrome.

Scripts live under **Automations → Scripts**. The language is **Grapheme**: GraphQL-style steps with Elixir-like pipes. You do **not** paste scripts into vault notes as fences — save them in the Scripts library, then run, add to a Flow, or schedule.

Related: [Automations and scripts](guide:grapheme-automations) · [Liquid blocks](guide:liquid-reference) · [Permissions](guide:permissions-budgets)

## Mental model

| Piece | What it is |
|-------|------------|
| **Script** | A reusable program you write and save |
| **Flow** | Ordered steps (Script, Ask Medousa, External tool) |
| **Schedule** | When a flow or prompt runs |
| **Agent** | An imported skill with its own tools — different from a script you author |

Automation **output** can show up in a note later via a Liquid **`feed`** block (last-good). See [Liquid reference — Feed](guide:liquid-reference#feed).

```callout
tone: tip
title: Start from a Template
body: In the Scripts workbench left rail, open Templates — Say hello, Search the web, Chain steps, Run a sandboxed command. Run once, then rewrite.
```

## Language basics

Scripts are built from a few ideas:

- **`glyph`** — a short named entry point (great for simple “do this” scripts).
- **`query`** — a named block of steps (good when you set values and pipe them).
- **`set { … }`** — put values into the current bag.
- **`|>`** — pass the current bag into the next step.
- **`$current.field`** — read a field from the current bag.
- **`// comment`** — notes for future-you (and for Medousa when a script is attached in chat).

Module calls look like `module.op(arg: value)`. Many ops let you pick fields in curly braces after the call.

## Starter examples

These match the Templates in Automations. Copy them into the Scripts workbench, **Save**, then **Run** (⌘/Ctrl+Enter).

### Say hello

Print a short message with `core.echo`.

```grapheme
glyph Main {
  core.echo(message: "Hello from Medousa!")
}
```

### Search the web

DuckDuckGo search — top results.

```grapheme
glyph Main {
  web.duckduckgo(query: "What's new in AI this week?", max_results: 3)
}
```

### Chain steps together

Set a value, then pipe it into the next step with `$current`.

```grapheme
query Demo on Any {
  set { message: "You wrote a real Grapheme script" }
    |> core.echo(message: $current.message)
}
```

### Run a sandboxed command

`shell.run` executes in the sandbox. Ask only for the fields you need.

```grapheme
query ShellEcho {
  shell.run(
    command: "echo hello from medousa",
    network: false,
    timeout_ms: 5000
  ) { exit_code stdout stderr backend sandboxed }
}
```

```callout
tone: warn
title: Shell and network need a healthy workshop
body: Sandboxed commands and web ops fail quietly if the workshop is offline. Check the status bar and Settings → Workshop before debugging the script.
```

## Useful modules

You don’t need the full catalog. Discover ops from the **Modules** / **WASM** rail (insert a call), or start from a Template and swap the op.

| Module | Typical use |
|--------|-------------|
| **core** | Echo, small transforms, glue |
| **web** / **websearch** | Search and research-style fetches |
| **http** | Fetch a URL |
| **html** | Turn HTML into markdown |
| **json** / **csv** | Parse structured text |
| **shell** | Sandboxed OS command |
| **medousa** | Digest / one-shot help / deliver into Medousa channels |

Prefer the smallest script that proves the next step. Long scripts before a successful **Run** are harder to debug.

## Save, run, compile

```steps
title: First successful script

---
label: Open Scripts
body: Automations → Scripts — pick a Template or New
status: current
---
label: Edit and Save
body: Name it after the job (⌘/Ctrl+S)
status: pending
---
label: Run
body: ⌘/Ctrl+Enter — read the output pane before assuming the editor is wrong
status: pending
---
label: Optional Compile
body: ⌘/Ctrl+B when you want a quick syntax check; Optimize (AOT) when the button is available
status: pending
---
label: Use it
body: Add to flow, schedule, or attach the script as chat context
status: pending
```

Operator tips:

- Keep scripts small and named after the job.
- Run by hand once before scheduling.
- Comment intent for future-you and for agents that see the script as context.

## Next

- [Automations and scripts](guide:grapheme-automations) — Flows, Schedules, History
- [Liquid blocks](guide:liquid-reference) — show last-good output with `feed`
- [Permissions, budgets, and tool safety](guide:permissions-budgets) — Allow / Deny when tools ask
