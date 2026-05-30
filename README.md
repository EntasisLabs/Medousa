# Medousa

**The evidence-aware AI workspace.**

Medousa is a permanent, local AI workspace built for people who demand data integrity. It combines a lightning-fast terminal UI with a persistent background engine that extracts, verifies, and traces the lineage of every answer it provides.

No hallucinations. Just verifiable proof.

### Get Started in 60 Seconds


```
# 1. Install the runtime
cargo install --path .

# 2. Run the interactive onboarding wizard
medousa setup
```

The `medousa setup` command opens an interactive terminal interface that auto-detects your local Ollama setup or links your cloud providers, configures your database, and safely mounts your messaging adapters in one continuous, beautiful flow.

## The Workspace Toolkit

Once onboarding is finished, the entire platform is controlled via a single binary interface:

|**Command**|**Action**|
|---|---|
|`medousa tui`|Open your interactive workspace (auto-starts background engine if idle).|
|`medousa doctor`|Run an instant diagnostic on model reachability, key validity, and system health.|
|`medousa telegram`|Activate your secure Telegram bridge using your saved session credentials.|
|`medousa discord`|Launch your channel ingress bridge with automated rate-limiting.|

> **Why Evidence-First Matters:** Traditional chat interfaces give you flat strings. Medousa runs deep tool loops, captures immutable payloads, and explicitly tags every block of text in your workspace as `verified` or `provisional` alongside its core citation records.