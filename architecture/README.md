# Medousa Architecture Map

This directory documents Medousa as a runtime product.

It focuses on:

- executable surfaces and their boundaries
- state ownership (ephemeral UI, persisted user state, durable runtime state)
- runtime composition and orchestration behavior
- operational interaction flows across local and daemon modes

Start here for product-level usage:

- [../README.md](../README.md)

## Documents

1. [system-overview.md](system-overview.md)
2. [enterprise-architecture-and-flow-guide.md](enterprise-architecture-and-flow-guide.md)
3. [component-cli.md](component-cli.md)
4. [component-tui.md](component-tui.md)
5. [component-daemon.md](component-daemon.md)
6. [interaction-and-state-model.md](interaction-and-state-model.md)
7. [tui-performance-target-plan.md](tui-performance-target-plan.md)

## Primary Code Anchors

- `medousa/src/lib.rs`
- `medousa/src/tools.rs`
- `medousa/src/bin/medousa_cli.rs`
- `medousa/src/bin/medousa_tui.rs`
- `medousa/src/bin/medousa_daemon.rs`
- `medousa/src/session.rs`
- `medousa/src/events.rs`
- `medousa/src/daemon_api.rs`
