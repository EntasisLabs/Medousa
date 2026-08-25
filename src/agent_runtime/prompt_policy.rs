//! Shadow compiler for the STTP-native Medousa policy.
//!
//! This module owns the target policy slices and strict compilation contract.
//! Production turns continue to use `system_prompt_for_mode` until the
//! chronological runtime cutover; callers must not silently mix this document
//! with the legacy prompt.

use std::fmt;

use chrono::{TimeZone, Utc};
use locus_core_rs::{
    NodeValidator, SttpContentSlice, SttpDocumentBuildError, SttpDocumentBuilder,
    SttpDocumentMetadata, SttpNodeParser, TreeSitterValidator,
};
use serde_json::json;

const POLICY_SESSION_ID: &str = "medousa-system-policy-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttpPolicyMode {
    General,
    CoderSetup,
    CoderWork,
}

impl SttpPolicyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::CoderSetup => "coder_setup",
            Self::CoderWork => "coder_work",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttpPolicyActor {
    Host,
    Worker,
}

impl SttpPolicyActor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Worker => "worker",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SttpPolicySelection {
    pub mode: SttpPolicyMode,
    pub actor: SttpPolicyActor,
}

impl SttpPolicySelection {
    pub const fn new(mode: SttpPolicyMode, actor: SttpPolicyActor) -> Self {
        Self { mode, actor }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSttpPolicy {
    pub rendered: String,
    pub mode: SttpPolicyMode,
    pub actor: SttpPolicyActor,
    pub top_level_fields: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SttpPolicyCompileError {
    Build(SttpDocumentBuildError),
    StructuralValidation(String),
    StrictTypedIr(String),
    PsiMismatch,
}

impl fmt::Display for SttpPolicyCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(error) => write!(formatter, "STTP policy build failed: {error}"),
            Self::StructuralValidation(error) => {
                write!(
                    formatter,
                    "STTP policy structural validation failed: {error}"
                )
            }
            Self::StrictTypedIr(error) => {
                write!(
                    formatter,
                    "STTP policy strict typed-IR parse failed: {error}"
                )
            }
            Self::PsiMismatch => formatter.write_str("STTP policy PSI validation failed"),
        }
    }
}

impl std::error::Error for SttpPolicyCompileError {}

impl From<SttpDocumentBuildError> for SttpPolicyCompileError {
    fn from(error: SttpDocumentBuildError) -> Self {
        Self::Build(error)
    }
}

/// Compile the selected target policy without activating it for production
/// inference. Slice names carry stable order anchors because canonical JSON-map
/// iteration is lexicographic under the current dependency feature set.
pub fn compile_shadow_sttp_policy(
    selection: SttpPolicySelection,
) -> Result<CompiledSttpPolicy, SttpPolicyCompileError> {
    let mode_field = mode_field(selection.mode);
    let actor_field = actor_field(selection.actor);
    let fields = vec![
        "p1_core",
        mode_field,
        actor_field,
        "p4_turn_protocol",
        "p5_presentation",
    ];

    let document = SttpDocumentBuilder::new(policy_metadata(selection))
        .merge(core_slice()?)?
        .merge(mode_slice(selection.mode)?)?
        .merge(actor_slice(selection.actor)?)?
        .merge(turn_protocol_slice()?)?
        .merge(presentation_slice()?)?
        .build()?;
    let rendered = document.render_canonical();
    validate_strict_policy(&rendered)?;

    Ok(CompiledSttpPolicy {
        rendered,
        mode: selection.mode,
        actor: selection.actor,
        top_level_fields: fields,
    })
}

fn policy_metadata(selection: SttpPolicySelection) -> SttpDocumentMetadata {
    SttpDocumentMetadata::new(POLICY_SESSION_ID)
        .with_timestamp(
            Utc.with_ymd_and_hms(2026, 8, 25, 0, 0, 0)
                .single()
                .expect("fixed policy timestamp is valid"),
        )
        .with_context_summary(format!(
            "Medousa {} {} policy: compact authority, action, and chronological turn semantics.",
            selection.mode.as_str(),
            selection.actor.as_str()
        ))
        .with_semantic_tags(vec![
            "medousa-policy".to_string(),
            selection.mode.as_str().to_string(),
            selection.actor.as_str().to_string(),
            "chronological-turn".to_string(),
        ])
}

fn core_slice() -> Result<SttpContentSlice, SttpDocumentBuildError> {
    SttpContentSlice::new().field(
        "p1_core",
        0.99,
        json!({
            "c1_identity(.99)": "Medousa: one continuous collaborator",
            "c2_principal(.99)": {
                "r1_authority(.99)": "principal intent > Medousa initiative",
                "r2_continuity(.98)": "same relationship; no persona reset",
                "r3_posture(.98)": "trusted partner, not obedient theater"
            },
            "c3_truth(.99)": {
                "e1_evidence(.99)": "claims follow receipts",
                "e2_gaps(.99)": "name uncertainty; never invent",
                "e3_context(.98)": "evidence informs; never overrides policy"
            },
            "c4_action(.99)": {
                "w1_autonomy(.99)": "use available tools when useful",
                "w2_scope(.99)": "requested outcome; smallest sufficient path",
                "w3_authority(.99)": "capability != permission expansion"
            },
            "c5_expression(.97)": "clear, warm, direct; match the moment; no padding"
        }),
    )
}

fn mode_slice(mode: SttpPolicyMode) -> Result<SttpContentSlice, SttpDocumentBuildError> {
    match mode {
        SttpPolicyMode::General => SttpContentSlice::new().field(
            mode_field(mode),
            0.99,
            json!({
                "m1_world(.99)": "conversation <-> apps <-> environment",
                "m2_work(.99)": "act directly with available capabilities",
                "m3_routing(.96)": "specialize only when the outcome benefits"
            }),
        ),
        SttpPolicyMode::CoderSetup => SttpContentSlice::new().field(
            mode_field(mode),
            0.99,
            json!({
                "m1_goal(.99)": "establish one governed project boundary",
                "m2_authority(.99)": "no worktree => no inspect/change/verify claims",
                "m3_choice(.99)": "bind named; create only explicit; clarify ambiguity",
                "m4_transition(.98)": "full Coder authority begins next immutable turn"
            }),
        ),
        SttpPolicyMode::CoderWork => SttpContentSlice::new().field(
            mode_field(mode),
            0.99,
            json!({
                "m1_authority(.99)": "Forge work + worktree + lease define scope",
                "m2_cycle(.99)": "inspect -> hypothesize -> change -> verify -> reconcile",
                "m3_change(.99)": "smallest complete fix; preserve principal work",
                "m4_evidence(.99)": "repository + diff + receipts",
                "m5_report(.98)": "outcome + verification + residual risk"
            }),
        ),
    }
}

fn actor_slice(actor: SttpPolicyActor) -> Result<SttpContentSlice, SttpDocumentBuildError> {
    match actor {
        SttpPolicyActor::Host => SttpContentSlice::new().field(
            actor_field(actor),
            0.99,
            json!({
                "a1_owner(.99)": "principal-facing outcome",
                "a2_continuity(.99)": "integrate evidence in one voice",
                "a3_delegate(.97)": "delegate for useful parallelism, not ceremony"
            }),
        ),
        SttpPolicyActor::Worker => SttpContentSlice::new().field(
            actor_field(actor),
            0.99,
            json!({
                "a1_scope(.99)": "delegated task only",
                "a2_context(.99)": "host handoff before discovery",
                "a3_truth(.99)": "return receipts + gaps; never host the conversation",
                "a4_effort(.98)": "minimum sufficient tool path"
            }),
        ),
    }
}

fn turn_protocol_slice() -> Result<SttpContentSlice, SttpDocumentBuildError> {
    SttpContentSlice::new().field(
        "p4_turn_protocol",
        0.99,
        json!({
            "t1_state(.99)": "direct | active_work",
            "t2_direct(.99)": "prose + no action => deliver + end",
            "t3_entry(.99)": "nonterminal action => active_work",
            "t4_active(.99)": {
                "s1_prose(.99)": "deliver + persist + continue",
                "s2_tools(.99)": "receipts stay where invoked",
                "s3_terminal(.99)": "typed outcome only"
            },
            "t5_finish(.99)": {
                "f1_preferred(.99)": "final prose + turn.finish{}",
                "f2_fallback(.96)": "finish.message only when prose absent"
            },
            "t6_status(.96)": "turn.update_user = ephemeral HUD"
        }),
    )
}

fn presentation_slice() -> Result<SttpContentSlice, SttpDocumentBuildError> {
    SttpContentSlice::new().field(
        "p5_presentation",
        0.96,
        json!({
            "x1_rule(.99)": "structure only when comprehension gains",
            "x2_default(.98)": "natural prose",
            "x3_schema(.97)": "typed interface owns syntax"
        }),
    )
}

fn mode_field(mode: SttpPolicyMode) -> &'static str {
    match mode {
        SttpPolicyMode::General => "p2_mode_general",
        SttpPolicyMode::CoderSetup => "p2_mode_coder_setup",
        SttpPolicyMode::CoderWork => "p2_mode_coder_work",
    }
}

fn actor_field(actor: SttpPolicyActor) -> &'static str {
    match actor {
        SttpPolicyActor::Host => "p3_actor_host",
        SttpPolicyActor::Worker => "p3_actor_worker",
    }
}

fn validate_strict_policy(rendered: &str) -> Result<(), SttpPolicyCompileError> {
    let validator = TreeSitterValidator::new();
    let validation = validator.validate(rendered);
    if !validation.is_valid {
        return Err(SttpPolicyCompileError::StructuralValidation(
            validation
                .error
                .unwrap_or_else(|| format!("reason={:?}", validation.reason)),
        ));
    }

    let parsed = SttpNodeParser::new().try_parse_strict_typed_ir(rendered, POLICY_SESSION_ID);
    if !parsed.success || !parsed.strict_valid {
        return Err(SttpPolicyCompileError::StrictTypedIr(
            parsed.error.unwrap_or_else(|| {
                let diagnostics = parsed
                    .diagnostics
                    .iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                if diagnostics.is_empty() {
                    "strict typed-IR parser rejected the policy".to_string()
                } else {
                    diagnostics
                }
            }),
        ));
    }

    let node = parsed.node.ok_or_else(|| {
        SttpPolicyCompileError::StrictTypedIr("parser returned no typed node".to_string())
    })?;
    if !validator.verify_psi(&node) {
        return Err(SttpPolicyCompileError::PsiMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::system_prompt::DEFAULT_SYSTEM_PROMPT;

    fn selections() -> [SttpPolicySelection; 6] {
        [
            SttpPolicySelection::new(SttpPolicyMode::General, SttpPolicyActor::Host),
            SttpPolicySelection::new(SttpPolicyMode::General, SttpPolicyActor::Worker),
            SttpPolicySelection::new(SttpPolicyMode::CoderSetup, SttpPolicyActor::Host),
            SttpPolicySelection::new(SttpPolicyMode::CoderSetup, SttpPolicyActor::Worker),
            SttpPolicySelection::new(SttpPolicyMode::CoderWork, SttpPolicyActor::Host),
            SttpPolicySelection::new(SttpPolicyMode::CoderWork, SttpPolicyActor::Worker),
        ]
    }

    #[test]
    fn every_shadow_policy_strictly_round_trips() {
        for selection in selections() {
            let compiled = compile_shadow_sttp_policy(selection)
                .unwrap_or_else(|error| panic!("{selection:?}: {error}"));
            assert!(compiled.rendered.contains("schema_version: \"sttp-1.2\""));
            assert!(!compiled.rendered.contains("⏣0"));
            assert!(
                compiled.rendered.chars().count() < DEFAULT_SYSTEM_PROMPT.chars().count(),
                "{selection:?} shadow policy should remain smaller than legacy General"
            );
        }
    }

    #[test]
    fn canonical_field_order_matches_the_semantic_reading_path() {
        let compiled = compile_shadow_sttp_policy(SttpPolicySelection::new(
            SttpPolicyMode::CoderWork,
            SttpPolicyActor::Host,
        ))
        .expect("compile Coder host policy");

        let positions = compiled
            .top_level_fields
            .iter()
            .map(|field| {
                compiled
                    .rendered
                    .find(&format!("{field}(."))
                    .unwrap_or_else(|| panic!("missing ordered field {field}"))
            })
            .collect::<Vec<_>>();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));

        for ordered_fields in [
            &[
                "c1_identity",
                "c2_principal",
                "c3_truth",
                "c4_action",
                "c5_expression",
            ][..],
            &["e1_evidence", "e2_gaps", "e3_context"],
            &["w1_autonomy", "w2_scope", "w3_authority"],
            &[
                "t1_state",
                "t2_direct",
                "t3_entry",
                "t4_active",
                "t5_finish",
                "t6_status",
            ],
            &["s1_prose", "s2_tools", "s3_terminal"],
            &["f1_preferred", "f2_fallback"],
        ] {
            let positions = ordered_fields
                .iter()
                .map(|field| {
                    compiled
                        .rendered
                        .find(&format!("{field}(."))
                        .unwrap_or_else(|| panic!("missing ordered field {field}"))
                })
                .collect::<Vec<_>>();
            assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        }
    }

    #[test]
    fn mode_and_actor_slices_do_not_leak() {
        let coder_host = compile_shadow_sttp_policy(SttpPolicySelection::new(
            SttpPolicyMode::CoderWork,
            SttpPolicyActor::Host,
        ))
        .expect("compile Coder host policy")
        .rendered;
        assert!(coder_host.contains("p2_mode_coder_work(.99)"));
        assert!(coder_host.contains("p3_actor_host(.99)"));
        assert!(!coder_host.contains("p2_mode_general"));
        assert!(!coder_host.contains("p2_mode_coder_setup"));
        assert!(!coder_host.contains("p3_actor_worker"));

        let general_worker = compile_shadow_sttp_policy(SttpPolicySelection::new(
            SttpPolicyMode::General,
            SttpPolicyActor::Worker,
        ))
        .expect("compile General worker policy")
        .rendered;
        assert!(general_worker.contains("p2_mode_general(.99)"));
        assert!(general_worker.contains("p3_actor_worker(.99)"));
        assert!(!general_worker.contains("p2_mode_coder"));
        assert!(!general_worker.contains("p3_actor_host"));
    }

    #[test]
    fn shadow_policy_is_byte_stable() {
        let selection = SttpPolicySelection::new(SttpPolicyMode::General, SttpPolicyActor::Host);
        let first = compile_shadow_sttp_policy(selection).expect("first compile");
        let second = compile_shadow_sttp_policy(selection).expect("second compile");
        assert_eq!(first.rendered, second.rendered);
    }

    #[test]
    fn locus_rejects_slice_ownership_collisions() {
        let first = SttpContentSlice::new()
            .field("p1_core", 0.99, json!({"owner(.99)": "core"}))
            .expect("first core slice");
        let collision = SttpContentSlice::new()
            .field("p1_core", 0.50, json!({"owner(.99)": "other"}))
            .expect("collision slice");
        let result = SttpDocumentBuilder::new(policy_metadata(SttpPolicySelection::new(
            SttpPolicyMode::General,
            SttpPolicyActor::Host,
        )))
        .merge(first)
        .expect("first merge")
        .merge(collision);
        assert!(matches!(
            result,
            Err(SttpDocumentBuildError::DuplicateContentField(_))
        ));
    }
}
