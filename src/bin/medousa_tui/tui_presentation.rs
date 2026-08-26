//! TUI presentation helpers — structured tool timeline (P4).

use medousa::{
    session::ConversationTurn,
    turn_parts::{TurnPart, TurnPartsAccumulator},
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub fn format_tool_name(tool: &str) -> String {
    tool.strip_prefix("cognition_")
        .unwrap_or(tool)
        .replace('_', " ")
}

fn tool_status_glyph(status: &str) -> (&'static str, Color) {
    match status {
        "failed" => ("✗", Color::Red),
        "running" | "started" => ("⟳", Color::Yellow),
        _ => ("✓", Color::Green),
    }
}

pub fn chronological_parts(
    turn: &ConversationTurn,
    live_parts: Option<&TurnPartsAccumulator>,
) -> Option<Vec<TurnPart>> {
    let parts = live_parts
        .map(TurnPartsAccumulator::preview_parts)
        .filter(|items| !items.is_empty())
        .or_else(|| turn.parts.clone())?;
    parts
        .iter()
        .any(|part| {
            matches!(
                part,
                TurnPart::Text {
                    segment_id: Some(_),
                    ..
                }
            )
        })
        .then_some(parts)
}

pub fn render_tool_part_line(part: &TurnPart) -> Option<Line<'static>> {
    let TurnPart::ToolRun {
        tool_name,
        status,
        input_summary,
        output_summary,
        ..
    } = part
    else {
        return None;
    };
    let (glyph, color) = tool_status_glyph(status);
    let name = format_tool_name(tool_name);
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(format!("{glyph} "), Style::default().fg(color)),
        Span::styled(name, Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("  {input_summary}"),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    if let Some(summary) = output_summary
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        spans.push(Span::raw(" → "));
        spans.push(Span::styled(
            summary.to_string(),
            Style::default().fg(Color::Gray),
        ));
    }
    Some(Line::from(spans))
}

pub fn render_turn_tool_lines(
    turn: &ConversationTurn,
    live_parts: Option<&TurnPartsAccumulator>,
) -> Vec<Line<'static>> {
    let parts = resolve_tool_parts(turn, live_parts);
    if parts.is_empty() {
        if turn.tool_names.is_empty() {
            return Vec::new();
        }
        return vec![Line::from(Span::styled(
            format!("  tools: {}", turn.tool_names.join(", ")),
            Style::default().fg(Color::DarkGray),
        ))];
    }

    let mut lines = Vec::new();
    let mut current_round: Option<usize> = None;

    for part in parts {
        let TurnPart::ToolRun {
            tool_name,
            status,
            input_summary,
            output_summary,
            tool_round,
            ..
        } = part
        else {
            continue;
        };

        let round = tool_round.unwrap_or(1);
        if current_round != Some(round) {
            if current_round.is_some() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  round {round}"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )));
            current_round = Some(round);
        }

        let (glyph, color) = tool_status_glyph(&status);
        let name = format_tool_name(&tool_name);
        let mut spans = vec![
            Span::raw("  "),
            Span::styled(format!("{glyph} "), Style::default().fg(color)),
            Span::styled(name, Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("  {input_summary}"),
                Style::default().fg(Color::DarkGray),
            ),
        ];
        if let Some(summary) = output_summary.filter(|value| !value.trim().is_empty()) {
            spans.push(Span::raw(" → "));
            spans.push(Span::styled(summary, Style::default().fg(Color::Gray)));
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn resolve_tool_parts(
    turn: &ConversationTurn,
    live_parts: Option<&TurnPartsAccumulator>,
) -> Vec<TurnPart> {
    if let Some(parts) = turn.parts.as_ref().filter(|items| !items.is_empty()) {
        return parts
            .iter()
            .filter(|part| matches!(part, TurnPart::ToolRun { .. }))
            .cloned()
            .collect();
    }
    live_parts
        .filter(|acc| acc.has_pending_tool_runs())
        .map(TurnPartsAccumulator::preview_tool_runs)
        .unwrap_or_default()
        .into_iter()
        .filter(|part| matches!(part, TurnPart::ToolRun { .. }))
        .collect()
}

pub fn render_handoff_line(turn: &ConversationTurn) -> Option<Line<'static>> {
    let handoff = turn.parts.as_ref()?.iter().find_map(|part| {
        if let TurnPart::Handoff {
            handoff_kind,
            text,
            work_id,
        } = part
        {
            Some((handoff_kind.clone(), text.clone(), work_id.clone()))
        } else {
            None
        }
    })?;
    let (kind, text, work_id) = handoff;
    let work_hint = work_id.map(|id| format!(" · {id}")).unwrap_or_default();
    Some(Line::from(vec![
        Span::styled("  ↗ ", Style::default().fg(Color::Magenta)),
        Span::styled(
            format!("handoff ({kind}){work_hint}"),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {text}"), Style::default().fg(Color::Gray)),
    ]))
}

/// Settled status notes archived from explicit turn-progress events.
pub fn progress_notes(turn: &ConversationTurn) -> Vec<String> {
    turn.parts
        .as_ref()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| match part {
                    TurnPart::Progress { markdown } => {
                        let trimmed = markdown.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.to_string())
                        }
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn chronological_parts_preserve_text_tool_text_order() {
        let mut live = TurnPartsAccumulator::default();
        live.start_text_segment("segment-1", Some(1));
        live.append_text_segment("segment-1", "first");
        live.tool_started("run-1", "search", "query", 1);
        live.tool_finished("run-1", "failed", Some("offline".into()), vec![]);
        live.start_text_segment("segment-2", Some(2));
        live.append_text_segment("segment-2", "recovered");
        let turn = ConversationTurn::plain("assistant", String::new(), Utc::now(), vec![], None);

        let parts = chronological_parts(&turn, Some(&live)).expect("chronological parts");
        assert!(matches!(&parts[0], TurnPart::Text { markdown, .. } if markdown == "first"));
        assert!(matches!(&parts[1], TurnPart::ToolRun { status, .. } if status == "failed"));
        assert!(matches!(&parts[2], TurnPart::Text { markdown, .. } if markdown == "recovered"));
    }

    #[test]
    fn legacy_flat_text_keeps_legacy_presentation_path() {
        let turn =
            ConversationTurn::plain("assistant", "answer".to_string(), Utc::now(), vec![], None);
        assert!(chronological_parts(&turn, None).is_none());
    }
}
