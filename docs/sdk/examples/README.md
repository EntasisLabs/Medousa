# SDK examples

Runnable snippets (add to `crates/medousa-sdk/examples/` when wiring CI).

## Health check

```rust
use medousa_sdk::MedousaClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = MedousaClient::new("http://127.0.0.1:7419");
    let health = client.health().get().await?;
    println!("{:?}", health);
    Ok(())
}
```

## Enqueue ask job

```rust
use medousa_types::EnqueueAskRequest;

let job = client.jobs().enqueue_ask(&EnqueueAskRequest {
    channel: "api".into(),
    user_id: "demo".into(),
    text: "Summarize status".into(),
    ..Default::default()
}).await?;
```

## List presentations

```rust
use medousa_types::ArtifactListUiRequest;

let list = client.runtime().artifact_list_ui(&ArtifactListUiRequest {
    session_id: None,
    limit: 20,
    query: None,
}).await?;
```

## Interactive turn (start only)

```rust
use medousa_types::InteractiveTurnRequest;

let ticket = client.interactive().start_turn(&InteractiveTurnRequest {
    session_id: "demo".into(),
    prompt: "Hi".into(),
    ..Default::default()
}).await?;
println!("Open SSE: {}", ticket.stream_url);
```

See [interactive-streaming.md](interactive-streaming.md) for SSE parsing.
