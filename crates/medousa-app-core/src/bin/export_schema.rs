//! Export JSON Schema for the application chat contract.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use medousa_app_core::contract::{
    AgentPlacementEntry, AgentPlacementGroup, AskThreadBatch, BeginTurnRequest, ChatCommand,
    ChatItem, ChatItemRole, ChatLane, ChatRelationship, ChatRevision, ChatSegment, ChatSnapshot,
    ChatUpdate, ProposalFact, TranscriptCoordinate, TurnFact, WorkerLinkFact, WorkerPlacementFact,
    WorkerReturnFact,
};
use schemars::schema::RootSchema;
use schemars::schema_for;

macro_rules! export_type {
    ($map:expr, $ty:ty, $name:expr) => {
        $map.insert($name.to_string(), schema_for!($ty));
    };
}

fn main() {
    let mut schemas: BTreeMap<String, RootSchema> = BTreeMap::new();
    export_type!(schemas, ChatRevision, "ChatRevision");
    export_type!(schemas, ChatItemRole, "ChatItemRole");
    export_type!(schemas, ChatLane, "ChatLane");
    export_type!(schemas, TranscriptCoordinate, "TranscriptCoordinate");
    export_type!(schemas, ChatSegment, "ChatSegment");
    export_type!(schemas, ChatItem, "ChatItem");
    export_type!(schemas, ChatRelationship, "ChatRelationship");
    export_type!(schemas, ProposalFact, "ProposalFact");
    export_type!(schemas, WorkerPlacementFact, "WorkerPlacementFact");
    export_type!(schemas, WorkerLinkFact, "WorkerLinkFact");
    export_type!(schemas, TurnFact, "TurnFact");
    export_type!(schemas, AskThreadBatch, "AskThreadBatch");
    export_type!(schemas, WorkerReturnFact, "WorkerReturnFact");
    export_type!(schemas, AgentPlacementEntry, "AgentPlacementEntry");
    export_type!(schemas, AgentPlacementGroup, "AgentPlacementGroup");
    export_type!(schemas, ChatSnapshot, "ChatSnapshot");
    export_type!(schemas, BeginTurnRequest, "BeginTurnRequest");
    export_type!(schemas, ChatCommand, "ChatCommand");
    export_type!(schemas, ChatUpdate, "ChatUpdate");

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root");
    let out = root.join("sdk-contract/medousa-app-api.schema.json");
    let encoded = serde_json::to_string_pretty(&schemas).expect("schema json");
    fs::write(&out, format!("{encoded}\n")).unwrap_or_else(|error| {
        panic!("failed to write {}: {error}", out.display());
    });
    println!("wrote {}", out.display());
}
