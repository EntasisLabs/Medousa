//! Liquid UI builder (`cognition_ui_build`) — atomic verbs that chain.
//!
//! The runtime owns layout: each verb expands into scene ops. The model never
//! authors `plan_layout` trees. Every successful response returns handles + a
//! `next` allowlist so the next call piggybacks like an API builder session.
//!
//! Wire compatibility: responses still carry `ops` so existing `ui_scene` stream
//! forwarding (`scene_ops_from_tool_output`) paints without a new event type.

use std::collections::HashMap;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
#[cfg(test)]
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

pub const COGNITION_UI_BUILD: &str = "cognition_ui_build";
const COGNITION_UI_BUILD_ID: ToolId = ToolId::new(COGNITION_UI_BUILD);

pub const UI_BUILD_COGNITION_TOOLS: &[&str] = &[COGNITION_UI_BUILD];

pub fn is_ui_build_cognition_tool(name: &str) -> bool {
    name == COGNITION_UI_BUILD
}

/// Tools whose successful output may carry `ops` for `ui_scene` streaming.
pub fn is_ui_scene_stream_tool(name: &str) -> bool {
    crate::ui_scene_tools::is_ui_scene_cognition_tool(name) || is_ui_build_cognition_tool(name)
}

#[derive(Debug, Clone)]
struct NodeSnap {
    id: String,
    ty: String,
    props: Value,
    parent: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BuildSession {
    surface_id: String,
    rev: i64,
    scene_id: String,
    body_id: String,
    nodes: HashMap<String, NodeSnap>,
    /// parent → ordered child ids
    order: HashMap<String, Vec<String>>,
    sealed: bool,
    seq: u64,
}

impl BuildSession {
    fn new(surface_id: String) -> Self {
        let scene_id = format!("build:{}", Uuid::new_v4());
        let body_id = format!("{scene_id}:body");
        let mut nodes = HashMap::new();
        nodes.insert(
            body_id.clone(),
            NodeSnap {
                id: body_id.clone(),
                ty: "stack".into(),
                props: json!({ "direction": "v", "gap": "md" }),
                parent: scene_id.clone(),
            },
        );
        Self {
            surface_id,
            rev: 1,
            scene_id,
            body_id,
            nodes,
            order: HashMap::new(),
            sealed: false,
            seq: 0,
        }
    }

    fn next_id(&mut self, kind: &str) -> String {
        self.seq += 1;
        format!("{}:{}:{}", self.scene_id, kind, self.seq)
    }

    fn has(&self, id: &str) -> bool {
        self.nodes.contains_key(id)
    }

    fn add_child(&mut self, snap: NodeSnap) {
        let parent = snap.parent.clone();
        let id = snap.id.clone();
        self.order.entry(parent).or_default().push(id.clone());
        self.nodes.insert(id, snap);
    }

    fn children_of(&self, parent: &str) -> Vec<&NodeSnap> {
        self.order
            .get(parent)
            .into_iter()
            .flatten()
            .filter_map(|id| self.nodes.get(id))
            .collect()
    }

    fn snap_to_wire(snap: &NodeSnap) -> Value {
        json!({
            "id": snap.id,
            "type": snap.ty,
            "props": snap.props,
            "fillState": "ready",
            "owner": "agent",
        })
    }

    fn fill_op(&self, parent: &str) -> Option<Value> {
        let parent_snap = self.nodes.get(parent)?;
        let slot = match parent_snap.ty.as_str() {
            "section" => "content",
            "stack" => "children",
            "document" => "flow",
            _ => "children",
        };
        let nodes: Vec<Value> = self
            .children_of(parent)
            .into_iter()
            .map(Self::snap_to_wire)
            .collect();
        Some(json!({
            "op": "fill_slot",
            "nodeId": parent,
            "slot": slot,
            "nodes": nodes,
        }))
    }

    fn begin_ops(&self) -> Vec<Value> {
        vec![json!({
            "op": "plan_layout",
            "surfaceId": self.surface_id,
            "rev": self.rev,
            "root": {
                "id": self.scene_id,
                "type": "document",
                "props": {},
                "fillState": "streaming",
                "owner": "agent",
                "slots": {
                    "flow": [{
                        "id": self.body_id,
                        "type": "stack",
                        "props": { "direction": "v", "gap": "md" },
                        "fillState": "streaming",
                        "owner": "agent",
                        "slots": { "children": [] }
                    }]
                }
            }
        })]
    }

    fn next_verbs(&self, parent: &str) -> Vec<&'static str> {
        if self.sealed {
            return vec![];
        }
        let ty = self
            .nodes
            .get(parent)
            .map(|n| n.ty.as_str())
            .unwrap_or("stack");
        match ty {
            "section" => vec!["set_prose", "add_card", "add_actions", "done"],
            _ => vec![
                "set_prose",
                "add_section",
                "add_card",
                "add_actions",
                "done",
            ],
        }
    }

    fn ok_response(
        &self,
        verb: &str,
        ops: Vec<Value>,
        handles: Value,
        parent_for_next: &str,
        preview: &str,
    ) -> Value {
        let op_count = ops.len();
        json!({
            "ok": true,
            "verb": verb,
            "ops": ops,
            "op_count": op_count,
            "surface_id": self.surface_id,
            "rev": self.rev,
            "handles": handles,
            "next": self.next_verbs(parent_for_next),
            "preview": preview,
        })
    }
}

type SessionMap = Arc<RwLock<HashMap<String, BuildSession>>>;

pub fn register_ui_build_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> stasis::prelude::Result<()> {
    let sessions: SessionMap = Arc::new(RwLock::new(HashMap::new()));
    registry.register_typed_tool(CognitionUiBuildTool::new(turn_scope, sessions))?;
    Ok(())
}

pub struct CognitionUiBuildTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    sessions: SessionMap,
}

impl CognitionUiBuildTool {
    pub(crate) fn new(
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        sessions: SessionMap,
    ) -> Self {
        Self {
            turn_scope,
            sessions,
        }
    }

    async fn supports_ui(&self) -> bool {
        self.turn_scope
            .read()
            .await
            .as_ref()
            .is_some_and(|scope| scope.supports_ui_artifacts)
    }

    async fn turn_id(&self) -> StasisResult<String> {
        self.turn_scope
            .read()
            .await
            .as_ref()
            .map(|s| s.turn_correlation_id.clone())
            .ok_or_else(|| StasisError::PortFailure("no active turn scope".to_string()))
    }

    fn verb(input: &UiBuildInput) -> Option<&str> {
        input
            .verb
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
    }

    fn string_arg(value: Option<&str>) -> Option<&str> {
        value.map(str::trim).filter(|v| !v.is_empty())
    }

    fn require_parent(session: &BuildSession, parent: &str) -> StasisResult<()> {
        if session.has(parent) {
            Ok(())
        } else {
            Err(StasisError::PortFailure(format!(
                "unknown parent handle `{parent}` — use a handle returned by a previous build call"
            )))
        }
    }

    async fn begin(&self, input: &UiBuildInput) -> StasisResult<Value> {
        let scope = self
            .turn_scope
            .read()
            .await
            .clone()
            .ok_or_else(|| StasisError::PortFailure("no active turn scope".to_string()))?;
        let surface_id = Self::string_arg(input.surface_id.as_deref())
            .map(str::to_string)
            .unwrap_or_else(|| format!("chat:turn:{}", scope.turn_correlation_id));

        let session = BuildSession::new(surface_id);
        let ops = session.begin_ops();
        let body_id = session.body_id.clone();
        let scene_id = session.scene_id.clone();
        let response = session.ok_response(
            "begin",
            ops,
            json!({ "sceneId": scene_id, "bodyId": body_id.clone() }),
            &body_id,
            "Opened Liquid body stack — chain set_prose / add_section / add_card / done",
        );
        self.sessions
            .write()
            .await
            .insert(scope.turn_correlation_id, session);
        Ok(response)
    }

    async fn with_open_session<F>(&self, f: F) -> StasisResult<Value>
    where
        F: FnOnce(&mut BuildSession) -> StasisResult<Value>,
    {
        let turn_id = self.turn_id().await?;
        let mut map = self.sessions.write().await;
        let session = map.get_mut(&turn_id).ok_or_else(|| {
            StasisError::PortFailure(
                "no active Liquid build — call cognition_ui_build verb=begin first".to_string(),
            )
        })?;
        if session.sealed {
            return Err(StasisError::PortFailure(
                "build already done — start a new turn to build again".to_string(),
            ));
        }
        f(session)
    }

    async fn set_prose(&self, input: &UiBuildInput) -> StasisResult<Value> {
        let markdown = Self::string_arg(input.markdown.as_deref())
            .or_else(|| Self::string_arg(input.text.as_deref()))
            .ok_or_else(|| StasisError::PortFailure("set_prose requires markdown".to_string()))?
            .to_string();
        let parent_arg = Self::string_arg(input.parent.as_deref()).map(str::to_string);

        self.with_open_session(|session| {
            let parent_id = parent_arg.unwrap_or_else(|| session.body_id.clone());
            Self::require_parent(session, &parent_id)?;
            let id = session.next_id("prose");
            session.add_child(NodeSnap {
                id: id.clone(),
                ty: "prose".into(),
                props: json!({ "markdown": markdown }),
                parent: parent_id.clone(),
            });
            let ops = vec![session.fill_op(&parent_id).expect("parent exists")];
            Ok(session.ok_response(
                "set_prose",
                ops,
                json!({ "nodeId": id, "parentId": parent_id }),
                &parent_id,
                "Set prose",
            ))
        })
        .await
    }

    async fn add_section(&self, input: &UiBuildInput) -> StasisResult<Value> {
        let title = Self::string_arg(input.title.as_deref())
            .ok_or_else(|| StasisError::PortFailure("add_section requires title".to_string()))?
            .to_string();
        let subtitle = Self::string_arg(input.subtitle.as_deref()).map(str::to_string);
        let parent_arg = Self::string_arg(input.parent.as_deref()).map(str::to_string);

        self.with_open_session(|session| {
            let parent_id = parent_arg.unwrap_or_else(|| session.body_id.clone());
            Self::require_parent(session, &parent_id)?;
            let id = session.next_id("section");
            let mut props = json!({ "title": title });
            if let Some(sub) = subtitle {
                props["subtitle"] = json!(sub);
            }
            session.add_child(NodeSnap {
                id: id.clone(),
                ty: "section".into(),
                props,
                parent: parent_id.clone(),
            });
            let ops = vec![session.fill_op(&parent_id).expect("parent exists")];
            Ok(session.ok_response(
                "add_section",
                ops,
                json!({ "sectionId": id, "parentId": parent_id }),
                &id,
                "Added section",
            ))
        })
        .await
    }

    async fn add_card(&self, input: &UiBuildInput) -> StasisResult<Value> {
        let title = Self::string_arg(input.title.as_deref())
            .ok_or_else(|| StasisError::PortFailure("add_card requires title".to_string()))?
            .to_string();
        let body = Self::string_arg(input.body.as_deref()).map(str::to_string);
        let subtitle = Self::string_arg(input.subtitle.as_deref()).map(str::to_string);
        let parent_arg = Self::string_arg(input.parent.as_deref()).map(str::to_string);

        self.with_open_session(|session| {
            let parent_id = parent_arg.unwrap_or_else(|| session.body_id.clone());
            Self::require_parent(session, &parent_id)?;
            let id = session.next_id("card");
            let mut props = json!({ "title": title });
            if let Some(b) = body {
                props["body"] = json!(b);
            }
            if let Some(s) = subtitle {
                props["subtitle"] = json!(s);
            }
            session.add_child(NodeSnap {
                id: id.clone(),
                ty: "card".into(),
                props,
                parent: parent_id.clone(),
            });
            let ops = vec![session.fill_op(&parent_id).expect("parent exists")];
            Ok(session.ok_response(
                "add_card",
                ops,
                json!({ "cardId": id, "parentId": parent_id }),
                &parent_id,
                "Added card",
            ))
        })
        .await
    }

    async fn add_actions(&self, input: &UiBuildInput) -> StasisResult<Value> {
        let actions = input.actions.as_ref().ok_or_else(|| {
            StasisError::PortFailure(
                "add_actions requires actions: [{ label, intent? }, ...]".to_string(),
            )
        })?;
        if actions.is_empty() {
            return Err(StasisError::PortFailure(
                "actions must not be empty".to_string(),
            ));
        }
        let parent_arg = Self::string_arg(input.parent.as_deref()).map(str::to_string);

        self.with_open_session(|session| {
            let parent_id = parent_arg.unwrap_or_else(|| session.body_id.clone());
            Self::require_parent(session, &parent_id)?;
            let mut last_id = parent_id.clone();
            for (i, action) in actions.iter().enumerate() {
                let label = action
                    .label
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        StasisError::PortFailure(format!("actions[{i}] requires label"))
                    })?;
                let intent = action
                    .intent
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .unwrap_or(label);
                let id = session.next_id("action");
                session.add_child(NodeSnap {
                    id: id.clone(),
                    ty: "action_row".into(),
                    props: json!({ "label": label, "intent": intent, "chevron": true }),
                    parent: parent_id.clone(),
                });
                last_id = id;
            }
            let ops = vec![session.fill_op(&parent_id).expect("parent exists")];
            Ok(session.ok_response(
                "add_actions",
                ops,
                json!({ "lastActionId": last_id, "parentId": parent_id }),
                &parent_id,
                "Added action row(s)",
            ))
        })
        .await
    }

    async fn done(&self) -> StasisResult<Value> {
        self.with_open_session(|session| {
            session.sealed = true;
            let ops = vec![json!({
                "op": "set_fill_state",
                "nodeId": session.scene_id,
                "state": "ready",
            })];
            Ok(session.ok_response(
                "done",
                ops,
                json!({ "sceneId": session.scene_id, "bodyId": session.body_id }),
                &session.body_id,
                "Liquid body sealed",
            ))
        })
        .await
    }
}

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum UiBuildVerbSchema {
    Begin,
    SetProse,
    AddSection,
    AddCard,
    AddActions,
    Done,
}

#[derive(Debug, JsonSchema)]
struct UiBuildActionInput {
    #[schemars(required, with = "String")]
    label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    intent: Option<String>,
}

#[derive(Debug, JsonSchema)]
pub struct UiBuildInput {
    /// Atomic builder verb. Chain begin → add_* / set_* → done.
    #[schemars(required, with = "UiBuildVerbSchema")]
    verb: Option<String>,
    /// Parent handle from a previous response (bodyId, sectionId, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    /// Prose markdown for set_prose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    markdown: Option<String>,
    /// Alias for markdown (set_prose).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    /// Title for add_section / add_card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    /// Optional subtitle for add_section / add_card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    subtitle: Option<String>,
    /// Optional body text for add_card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    /// For add_actions: [{ label, intent? }, ...]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "Vec<UiBuildActionInput>",
        skip_serializing_if = "Option::is_none"
    )]
    actions: Option<Vec<UiBuildActionInput>>,
    /// Optional surface id for begin (defaults to chat:turn:<turnId>).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    surface_id: Option<String>,
}

impl<'de> Deserialize<'de> for UiBuildInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let string = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let actions = value.get("actions").and_then(Value::as_array).map(|items| {
            items
                .iter()
                .map(|item| UiBuildActionInput {
                    label: item
                        .get("label")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    intent: item
                        .get("intent")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
                .collect()
        });
        Ok(Self {
            verb: string("verb").or_else(|| string("op")),
            parent: string("parent"),
            markdown: string("markdown"),
            text: string("text"),
            title: string("title"),
            subtitle: string("subtitle"),
            body: string("body"),
            actions,
            surface_id: string("surface_id"),
        })
    }
}

#[medousa_tool(id = COGNITION_UI_BUILD_ID)]
impl CognitionUiBuildTool {
    /// Build a streaming interactive Liquid UI body with atomic verbs that chain. Prefer markdown embeds (```card``` / ```carousel``` / ```actions``` / {{icon:}}) for ordinary structured chat answers; use this tool when you need a multi-step scene session. Preferred over cognition_ui_scene — you never invent layout trees. Call verb=begin first; each response returns handles + next[] for the following call. Verbs: begin, set_prose, add_section, add_card, add_actions, done. Pass parent= from a prior handles.* id. Runtime expands verbs into scene ops.
    async fn invoke_typed(&self, input: UiBuildInput) -> stasis::prelude::Result<ExternalJson> {
        if !self.supports_ui().await {
            return Ok(ExternalJson::new(json!({
                "ok": false,
                "unsupported_surface": true,
                "error": "This channel does not support UI scenes (supports_ui_artifacts=false). Answer in markdown instead.",
            })));
        }

        let verb = Self::verb(&input).ok_or_else(|| {
            StasisError::PortFailure(
                "verb is required (begin|set_prose|add_section|add_card|add_actions|done)"
                    .to_string(),
            )
        })?;

        let output = match verb {
            "begin" => self.begin(&input).await,
            "set_prose" => self.set_prose(&input).await,
            "add_section" => self.add_section(&input).await,
            "add_card" => self.add_card(&input).await,
            "add_actions" => self.add_actions(&input).await,
            "done" => self.done().await,
            other => Err(StasisError::PortFailure(format!(
                "unknown verb `{other}` — use begin|set_prose|add_section|add_card|add_actions|done"
            ))),
        }?;
        Ok(ExternalJson::new(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_api::TurnSurfaceContext;

    fn scope(supports: bool) -> Arc<RwLock<Option<TurnContinuationScope>>> {
        Arc::new(RwLock::new(Some(TurnContinuationScope {
            turn_correlation_id: "turn-1".to_string(),
            session_id: "medousa-home".to_string(),
            original_prompt: "hi".to_string(),
            delivery_target: None,
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            response_depth_mode: "standard".to_string(),
            supports_ui_artifacts: supports,
            supports_liquid_markdown: supports,
            supports_browser_host: false,
            channel_surface: Some("home-desktop".to_string()),
        })))
    }

    fn tool(supports: bool) -> CognitionUiBuildTool {
        CognitionUiBuildTool::new(scope(supports), Arc::new(RwLock::new(HashMap::new())))
    }

    #[test]
    fn surface_context_flag_default_is_false() {
        assert!(!TurnSurfaceContext::tui().supports_ui_artifacts);
    }

    #[tokio::test]
    async fn rejects_unsupported_surface() {
        let tool = tool(false);
        let out = tool
            .invoke(json!({ "verb": "begin" }))
            .await
            .expect("invoke");
        assert_eq!(out.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            out.get("unsupported_surface").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[tokio::test]
    async fn begin_then_set_prose_then_done_chain() {
        let tool = tool(true);
        let begin = tool
            .invoke(json!({ "verb": "begin" }))
            .await
            .expect("begin");
        assert_eq!(begin.get("ok").and_then(Value::as_bool), Some(true));
        assert!(
            begin
                .get("ops")
                .and_then(Value::as_array)
                .is_some_and(|o| !o.is_empty())
        );
        let body_id = begin["handles"]["bodyId"]
            .as_str()
            .expect("bodyId")
            .to_string();
        let next = begin["next"].as_array().expect("next");
        assert!(next.iter().any(|v| v.as_str() == Some("set_prose")));

        let prose = tool
            .invoke(json!({
                "verb": "set_prose",
                "parent": body_id,
                "markdown": "Hello **world**"
            }))
            .await
            .expect("set_prose");
        assert_eq!(prose.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(prose["preview"].as_str(), Some("Set prose"));

        let done = tool.invoke(json!({ "verb": "done" })).await.expect("done");
        assert_eq!(done.get("ok").and_then(Value::as_bool), Some(true));
        assert!(done["ops"][0]["op"].as_str() == Some("set_fill_state"));
    }

    #[tokio::test]
    async fn fill_without_begin_fails() {
        let tool = tool(true);
        let err = tool
            .invoke(json!({ "verb": "set_prose", "markdown": "x" }))
            .await
            .expect_err("need begin");
        assert!(err.to_string().contains("begin"));
    }

    #[tokio::test]
    async fn rejects_unknown_parent() {
        let tool = tool(true);
        tool.invoke(json!({ "verb": "begin" }))
            .await
            .expect("begin");
        let err = tool
            .invoke(json!({
                "verb": "add_card",
                "parent": "invented-id",
                "title": "Nope"
            }))
            .await
            .expect_err("bad parent");
        assert!(err.to_string().contains("unknown parent"));
    }

    #[tokio::test]
    async fn add_section_and_card_chain() {
        let tool = tool(true);
        let begin = tool
            .invoke(json!({ "verb": "begin" }))
            .await
            .expect("begin");
        let body_id = begin["handles"]["bodyId"].as_str().unwrap().to_string();
        let section = tool
            .invoke(json!({
                "verb": "add_section",
                "parent": body_id,
                "title": "Models"
            }))
            .await
            .expect("section");
        let section_id = section["handles"]["sectionId"]
            .as_str()
            .unwrap()
            .to_string();
        let card = tool
            .invoke(json!({
                "verb": "add_card",
                "parent": section_id,
                "title": "Mythos",
                "body": "Frontier"
            }))
            .await
            .expect("card");
        assert_eq!(card.get("ok").and_then(Value::as_bool), Some(true));
        assert!(card["ops"][0]["op"].as_str() == Some("fill_slot"));
    }
}
