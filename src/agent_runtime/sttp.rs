//! Structural validation for canonical STTP nodes used by runtime prompts.

const BLOCK_MARKERS: [&str; 4] = ["⊕⟨", "⦿⟨", "◈⟨", "⍉⟨"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SttpValidationError(pub String);

impl std::fmt::Display for SttpValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SttpValidationError {}

/// Validate the canonical four-block STTP shape used for mode policy and
/// world-state nodes. This deliberately validates structure rather than
/// attempting to interpret repository/user strings embedded as values.
pub fn validate_canonical_sttp_node(node: &str) -> Result<(), SttpValidationError> {
    let node = node.trim();
    if !node.starts_with(BLOCK_MARKERS[0]) || !node.ends_with('⟩') {
        return Err(SttpValidationError(
            "STTP node must contain no prose outside its canonical blocks".into(),
        ));
    }

    let mut positions = Vec::with_capacity(BLOCK_MARKERS.len());
    for marker in BLOCK_MARKERS {
        let marker_positions = unquoted_marker_positions(node, marker);
        if marker_positions.len() != 1 {
            return Err(SttpValidationError(format!(
                "STTP node must contain exactly one {marker} block"
            )));
        }
        positions.push(marker_positions[0]);
    }
    if !positions.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(SttpValidationError(
            "STTP blocks must be ordered provenance → envelope → content → metrics".into(),
        ));
    }
    if unquoted_marker_positions(node, "⏣0{").len() != 4 {
        return Err(SttpValidationError(
            "every canonical STTP block must scope the same ⏣0 node".into(),
        ));
    }

    let provenance = &node[positions[0]..positions[1]];
    require_fields(
        "provenance",
        provenance,
        &[
            "trigger:",
            "response_format:",
            "origin_session:",
            "compression_depth:",
            "parent_node:",
            "prime:",
            "attractor_config:",
            "context_summary:",
            "relevant_tier:",
            "retrieval_budget:",
        ],
    )?;
    if !["seed", "manual", "scheduled", "threshold", "resonance"]
        .iter()
        .any(|trigger| provenance.contains(&format!("trigger: {trigger}")))
    {
        return Err(SttpValidationError(
            "STTP provenance uses a non-canonical trigger".into(),
        ));
    }
    if !provenance.contains("response_format: temporal_node") {
        return Err(SttpValidationError(
            "runtime mode STTP must use response_format: temporal_node".into(),
        ));
    }
    if !provenance.contains("parent_node: null") && !provenance.contains("parent_node: ref:⏣") {
        return Err(SttpValidationError(
            "STTP parent_node must be null or a ref:⏣N lineage reference".into(),
        ));
    }
    require_fields(
        "provenance attractor_config",
        provenance,
        &["stability:", "friction:", "logic:", "autonomy:"],
    )?;
    require_fields(
        "envelope",
        &node[positions[1]..positions[2]],
        &[
            "timestamp:",
            "tier:",
            "session_id:",
            "schema_version:",
            "user_avec:",
            "model_avec:",
        ],
    )?;
    let envelope = &node[positions[1]..positions[2]];
    if !envelope.contains("schema_version: \"sttp-1.0\"") {
        return Err(SttpValidationError(
            "runtime mode STTP requires schema_version sttp-1.0".into(),
        ));
    }
    for vector in ["user_avec:", "model_avec:"] {
        let start = envelope
            .find(vector)
            .expect("required envelope vector was checked");
        require_fields(
            vector.trim_end_matches(':'),
            &envelope[start..],
            &["stability:", "friction:", "logic:", "autonomy:", "psi:"],
        )?;
    }
    let content = &node[positions[2]..positions[3]];
    if !has_confidence_weighted_field(content) {
        return Err(SttpValidationError(
            "STTP content requires field_name(.confidence): value fields".into(),
        ));
    }
    require_fields(
        "metrics",
        &node[positions[3]..],
        &["rho:", "kappa:", "psi:", "compression_avec:"],
    )?;
    let metrics = &node[positions[3]..];
    let compression_start = metrics
        .find("compression_avec:")
        .expect("required metrics vector was checked");
    require_fields(
        "compression_avec",
        &metrics[compression_start..],
        &["stability:", "friction:", "logic:", "autonomy:", "psi:"],
    )?;

    for (index, start) in positions.iter().copied().enumerate() {
        let end = positions.get(index + 1).copied().unwrap_or(node.len());
        if !node[start..end].trim_end().ends_with("} ⟩") {
            return Err(SttpValidationError(format!(
                "{} STTP block is not canonically closed",
                ["provenance", "envelope", "content", "metrics"][index]
            )));
        }
    }
    Ok(())
}

fn unquoted_marker_positions(value: &str, marker: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if in_string && escaped {
            escaped = false;
            continue;
        }
        if in_string && character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string && value[index..].starts_with(marker) {
            positions.push(index);
        }
    }
    positions
}

fn require_fields(
    block_name: &str,
    block: &str,
    fields: &[&str],
) -> Result<(), SttpValidationError> {
    if let Some(field) = fields.iter().find(|field| !block.contains(**field)) {
        return Err(SttpValidationError(format!(
            "STTP {block_name} block is missing {field}"
        )));
    }
    Ok(())
}

fn has_confidence_weighted_field(content: &str) -> bool {
    content.lines().any(|line| {
        let Some(open) = line.find("(.") else {
            return false;
        };
        let Some(close) = line[open + 2..].find("):") else {
            return false;
        };
        let confidence = &line[open + 2..open + 2 + close];
        !confidence.is_empty()
            && confidence
                .chars()
                .all(|character| character.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"⊕⟨ ⏣0{ trigger: seed, response_format: temporal_node, origin_session: "test", compression_depth: 1, parent_node: null, prime: { attractor_config: { stability: 0.9, friction: 0.2, logic: 0.9, autonomy: 0.8 }, context_summary: "test", relevant_tier: raw, retrieval_budget: 4 } } ⟩
⦿⟨ ⏣0{ timestamp: "2026-08-03T00:00:00Z", tier: raw, session_id: "test", schema_version: "sttp-1.0", user_avec: { stability: 0.9, friction: 0.2, logic: 0.9, autonomy: 0.8, psi: 2.8 }, model_avec: { stability: 0.9, friction: 0.2, logic: 0.9, autonomy: 0.8, psi: 2.8 } } ⟩
◈⟨ ⏣0{ role(.99): "test" } ⟩
⍉⟨ ⏣0{ rho: 0.9, kappa: 0.9, psi: 2.8, compression_avec: { stability: 0.9, friction: 0.2, logic: 0.9, autonomy: 0.8, psi: 2.8 } } ⟩"#;

    #[test]
    fn accepts_a_complete_canonical_node() {
        validate_canonical_sttp_node(VALID).expect("valid STTP");
    }

    #[test]
    fn rejects_markdown_and_missing_blocks() {
        let error = validate_canonical_sttp_node("## Coder mode\nDo engineering.")
            .expect_err("markdown is not STTP");
        assert!(error.to_string().contains("no prose outside"));
    }

    #[test]
    fn rejects_out_of_order_blocks() {
        let malformed = VALID
            .replace("⦿⟨", "TMP⟨")
            .replace("◈⟨", "⦿⟨")
            .replace("TMP⟨", "◈⟨");
        assert!(validate_canonical_sttp_node(&malformed).is_err());
    }

    #[test]
    fn ignores_protocol_markers_inside_quoted_data() {
        let with_quoted_marker = VALID.replace(
            "role(.99): \"test\"",
            "role(.99): \"repository example: ⊕⟨ ⏣0{ not a protocol block\"",
        );
        validate_canonical_sttp_node(&with_quoted_marker).expect("quoted marker is data");
    }
}
