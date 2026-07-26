# Grapheme and automations

Grapheme is Medousa's scripting surface — code that can talk to the host (`shell.*`, `medousa.*`, and friends) with editor support, recipes, and a path into flows.

## Script workbench

Open Automations / Scripts from the rail. You get:

- An editor with Grapheme language support
- Completions for host modules
- Hover docs when the language server is connected
- Run output for the last invocation

Start from a **recipe** when you don't want a blank page. Recipes are opinionated starters, not cages — rewrite freely.

## Host modules

Host ops are how scripts reach the machine and the workshop. Prefer documented modules over raw shell when a first-class op exists. Incomplete or dangerous calls should fail loudly — read the run output before blaming the editor.

## Flows

A script can graduate into a **flow** when it becomes a repeatable automation (including scheduled work). Flows show up in automations lists and status attention counts when something needs you.

## Operator tips

1. Keep scripts small and named after the job.
2. Log intent in comments for future-you and for agents reading the vault copy.
3. Don't schedule a script you haven't run by hand once.

```callout
tone: warn
title: Automations still need a healthy workshop
body: Scheduled work fails quietly if the engine is offline. Check Workshops and connections when a cron never fires.
```

Related: [Workshops and connections](guide:workshops-connections), [Keyboard and flow](guide:keyboard-flow).
