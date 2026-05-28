# Component: medousa-cli

## Role in the Product

medousa-cli is the non-interactive control surface for Medousa.

Use it when you want:

- one-shot prompt/ask execution
- scripted automation in shell environments
- daemon interaction without opening the TUI

## Entry Point

- Binary: medousa/src/bin/medousa_cli.rs

## Command Categories

Local runtime commands:

- ask
- llm

Daemon client commands:

- daemon-health
- daemon-stats
- daemon-ask
- daemon-watch-add

## Local Execution Flow

For ask and llm commands, the CLI performs a full local runtime cycle:

1. resolve backend/provider/model/base-url from args/env defaults
2. build runtime composition via build_runtime(...)
3. construct workflow/prompt payload contract
4. build NewJob with Stasis workflow builder
5. enqueue job into runtime
6. process one cycle (process_once)
7. read attempt diagnostics and print output

This gives deterministic one-command behavior while still using durable runtime primitives.

## Daemon Client Flow

For daemon-* commands, CLI acts as an HTTP client against daemon endpoints:

- /health
- /v1/stats
- /v1/jobs/ask
- /v1/recurring/prompt

## State and Persistence Characteristics

CLI is intentionally near-stateless:

- no long-lived in-process application state
- no local session history owned by CLI
- durable state is owned by runtime backend and/or daemon stores

## Operational Expectations

- local commands return attempt-level diagnostics when available
- daemon commands return typed API responses or HTTP failure information
- behavior remains aligned with TUI/daemon because execution primitives are shared
