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
use medousa_app_core::shell::{
    EditorGroup, ShellCommand, ShellDesktop, ShellDesktopLayout, ShellRevision, ShellSnapshot,
    ShellTab, ShellUpdate, SplitEdge, SplitNode,
};
use medousa_app_core::undertaking_binding::{
    UndertakingBindingCommand, UndertakingBindingRevision, UndertakingBindingSnapshot,
    UndertakingBindingUpdate, UndertakingContextFact, UndertakingGroupBinding,
};
use medousa_app_core::vault_buffer::{
    NoteBufferFact, VaultBufferCommand, VaultBufferRevision, VaultBufferSnapshot, VaultBufferUpdate,
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
    export_type!(schemas, ShellRevision, "ShellRevision");
    export_type!(schemas, ShellTab, "ShellTab");
    export_type!(schemas, EditorGroup, "EditorGroup");
    export_type!(schemas, SplitEdge, "SplitEdge");
    export_type!(schemas, SplitNode, "SplitNode");
    export_type!(schemas, ShellDesktopLayout, "ShellDesktopLayout");
    export_type!(schemas, ShellDesktop, "ShellDesktop");
    export_type!(schemas, ShellSnapshot, "ShellSnapshot");
    export_type!(schemas, ShellCommand, "ShellCommand");
    export_type!(schemas, ShellUpdate, "ShellUpdate");
    export_type!(schemas, VaultBufferRevision, "VaultBufferRevision");
    export_type!(schemas, NoteBufferFact, "NoteBufferFact");
    export_type!(schemas, VaultBufferSnapshot, "VaultBufferSnapshot");
    export_type!(schemas, VaultBufferCommand, "VaultBufferCommand");
    export_type!(schemas, VaultBufferUpdate, "VaultBufferUpdate");
    export_type!(schemas, UndertakingBindingRevision, "UndertakingBindingRevision");
    export_type!(schemas, UndertakingContextFact, "UndertakingContextFact");
    export_type!(schemas, UndertakingGroupBinding, "UndertakingGroupBinding");
    export_type!(schemas, UndertakingBindingSnapshot, "UndertakingBindingSnapshot");
    export_type!(schemas, UndertakingBindingCommand, "UndertakingBindingCommand");
    export_type!(schemas, UndertakingBindingUpdate, "UndertakingBindingUpdate");

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
