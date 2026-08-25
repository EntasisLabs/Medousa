//! Portable ephemeral state for one foreground turn.

use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

const GOAL_DISPLAY_MAX_CHARS: usize = 480;
const WORKING_NOTES_MAX: usize = 5;
const WORKING_NOTE_MAX_CHARS: usize = 120;
const DIGESTS_RECENT_SHOWN: usize = 5;
const MAX_DIGESTS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnScratchPhase {
    #[default]
    Discover,
    Execute,
    Finalize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkerDelegateScratch {
    pub work_id: String,
    pub intent: String,
}

/// Ephemeral working memory for one host or worker tool-loop execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnScratchpad {
    pub goal: String,
    pub phase: TurnScratchPhase,
    pub step: usize,
    pub last_tools: Vec<String>,
    pub last_error: Option<String>,
    pub open_gaps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegate: Option<WorkerDelegateScratch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub round_digests: Vec<String>,
    /// Union of tool names invoked this turn (deduped, insertion order).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools_this_turn: Vec<String>,
    /// Agent-pinned sticky notes (via begin_work note= or runtime).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub working_notes: Vec<String>,
}

impl TurnScratchpad {
    pub fn from_user_prompt(user_prompt: &str) -> Self {
        Self {
            goal: infer_goal_from_prompt(user_prompt),
            phase: TurnScratchPhase::Discover,
            ..Default::default()
        }
    }

    pub fn set_goal(&mut self, goal: impl Into<String>) {
        let goal = goal.into();
        if !goal.trim().is_empty() {
            self.goal = goal;
        }
    }

    pub fn set_open_gaps(&mut self, gaps: &[String]) {
        self.open_gaps = gaps.to_vec();
    }

    pub fn push_working_note(&mut self, note: impl Into<String>) {
        let note = truncate_field(&note.into(), WORKING_NOTE_MAX_CHARS);
        if note.trim().is_empty() {
            return;
        }
        if self.working_notes.len() >= WORKING_NOTES_MAX {
            self.working_notes.remove(0);
        }
        self.working_notes.push(note);
    }

    pub fn accumulate_tools(&mut self, tool_names: &[String]) {
        for name in tool_names {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !self
                .tools_this_turn
                .iter()
                .any(|existing| existing == trimmed)
            {
                self.tools_this_turn.push(trimmed.to_string());
            }
        }
    }

    pub fn set_delegate(&mut self, work_id: impl Into<String>, intent: impl Into<String>) {
        self.delegate = Some(WorkerDelegateScratch {
            work_id: work_id.into(),
            intent: intent.into(),
        });
        self.phase = TurnScratchPhase::Finalize;
    }

    pub fn on_tool_round_start(&mut self, round: usize) {
        self.step = round;
        if self.phase == TurnScratchPhase::Discover {
            self.phase = TurnScratchPhase::Execute;
        }
    }

    pub fn record_round_digest(&mut self, tool_results: &[(String, bool)]) {
        let names: Vec<String> = tool_results.iter().map(|(name, _)| name.clone()).collect();
        let entries: Vec<String> = tool_results
            .iter()
            .map(|(name, ok)| format_tool_digest_entry(name, *ok, None))
            .collect();
        self.record_round_digest_entries(&names, &entries);
    }

    /// Record a precompiled digest while keeping state limits centralized.
    pub fn record_round_digest_entries(
        &mut self,
        tool_names: &[String],
        digest_entries: &[String],
    ) {
        self.accumulate_tools(tool_names);
        self.last_tools = tool_names.to_vec();
        if let Some(name) = tool_names
            .iter()
            .zip(digest_entries.iter())
            .find(|(_, entry)| entry.contains(":fail"))
            .map(|(name, _)| name.clone())
        {
            self.last_error = Some(format!("{name} returned ok=false"));
        } else {
            self.last_error = None;
        }
        self.round_digests.push(format!(
            "round={} tools=[{}]",
            self.step,
            digest_entries.join(", ")
        ));
        if self.round_digests.len() > MAX_DIGESTS {
            let drain = self.round_digests.len() - MAX_DIGESTS;
            self.round_digests.drain(0..drain);
        }
    }

    pub fn format_control_body(&self, tool_rounds_remaining: usize) -> String {
        let phase = match self.phase {
            TurnScratchPhase::Discover => "discover",
            TurnScratchPhase::Execute => "execute",
            TurnScratchPhase::Finalize => "finalize",
        };
        let mut lines = vec![
            format!(
                "goal={}",
                truncate_field(&self.goal, GOAL_DISPLAY_MAX_CHARS)
            ),
            format!(
                "phase={phase} step={} rounds_remaining={tool_rounds_remaining}",
                self.step
            ),
        ];
        if !self.tools_this_turn.is_empty() {
            lines.push(format!(
                "tools_this_turn={}",
                self.tools_this_turn.join(", ")
            ));
        } else if !self.last_tools.is_empty() {
            lines.push(format!("last_tools={}", self.last_tools.join(", ")));
        }
        if let Some(error) = self.last_error.as_deref() {
            lines.push(format!("last_error={error}"));
        }
        if !self.open_gaps.is_empty() {
            lines.push(format!("open_gaps={}", self.open_gaps.join(", ")));
        }
        if !self.working_notes.is_empty() {
            lines.push(format!("working_notes={}", self.working_notes.join(" | ")));
        }
        if let Some(delegate) = self.delegate.as_ref() {
            lines.push(format!(
                "delegate work_id={} intent={}",
                delegate.work_id, delegate.intent
            ));
        }
        if !self.round_digests.is_empty() {
            let start = self
                .round_digests
                .len()
                .saturating_sub(DIGESTS_RECENT_SHOWN);
            lines.push(format!(
                "digests_recent={}",
                self.round_digests[start..].join(" · ")
            ));
        }
        lines.join("\n")
    }

    pub fn digest_hash(&self) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.goal.hash(&mut hasher);
        self.step.hash(&mut hasher);
        for digest in &self.round_digests {
            digest.hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }
}

fn format_tool_digest_entry(name: &str, ok: bool, hint: Option<&str>) -> String {
    let status = if ok { "ok" } else { "fail" };
    match hint.filter(|value| !value.trim().is_empty()) {
        Some(hint) => format!("{name}:{status} ({hint})"),
        None => format!("{name}:{status}"),
    }
}

fn infer_goal_from_prompt(user_prompt: &str) -> String {
    let collapsed = user_prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_field(&collapsed, GOAL_DISPLAY_MAX_CHARS)
}

fn truncate_field(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratchpad_records_tools_errors_notes_and_bounded_digests() {
        let mut scratch = TurnScratchpad::from_user_prompt("  inspect   the runtime  ");
        scratch.on_tool_round_start(1);
        scratch.record_round_digest(&[("read".to_string(), true), ("compile".to_string(), false)]);
        scratch.push_working_note("preserve the one-loop invariant");

        assert_eq!(scratch.goal, "inspect the runtime");
        assert_eq!(scratch.phase, TurnScratchPhase::Execute);
        assert_eq!(scratch.tools_this_turn, ["read", "compile"]);
        assert_eq!(
            scratch.last_error.as_deref(),
            Some("compile returned ok=false")
        );
        assert!(
            scratch
                .format_control_body(4)
                .contains("rounds_remaining=4")
        );
        assert!(
            scratch
                .format_control_body(4)
                .contains("working_notes=preserve the one-loop invariant")
        );
    }

    #[test]
    fn scratchpad_wire_shape_keeps_full_runtime_fields() {
        let mut scratch = TurnScratchpad::default();
        scratch.tools_this_turn.push("query".to_string());
        scratch.working_notes.push("note".to_string());
        let value = serde_json::to_value(scratch).unwrap();
        assert_eq!(value["tools_this_turn"][0], "query");
        assert_eq!(value["working_notes"][0], "note");
    }
}
