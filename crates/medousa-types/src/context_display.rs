use crate::daemon_api::ContextUsageReport;

pub fn operator_summary(report: &ContextUsageReport) -> String {
    let total = format_tokens(report.total_tokens_estimate);
    let limit = report
        .context_limit_tokens
        .map(|limit| format!(" / {}", format_tokens(limit)))
        .unwrap_or_default();
    let system = report
        .layers
        .iter()
        .find(|l| l.id == "system_prompt")
        .map(|l| format_tokens(l.tokens_estimate))
        .unwrap_or_else(|| "?".to_string());
    let tools = report
        .layers
        .iter()
        .find(|l| l.id == "tool_definitions")
        .map(|l| format_tokens(l.tokens_estimate))
        .unwrap_or_else(|| "?".to_string());
    let accounting = format!("Context ~{total}{limit} · system ~{system} · tools ~{tools}");
    let Some(hud) = &report.hud else {
        return accounting;
    };
    let mut parts = Vec::new();
    if let Some(work) = &hud.work {
        parts.push(format!(
            "Work {}: {} (authoritative lifecycle, not task verification)",
            work.work_id, work.phase
        ));
        if let Some(progress) = &work.progress {
            parts.push(progress.clone());
        }
        parts.extend(work.blockers.iter().map(|b| format!("Blocker: {b}")));
        parts.extend(
            work.decisions
                .iter()
                .map(|d| format!("Decision accounting: {d}")),
        );
    }
    if hud.deferred_items > 0 {
        parts.push(format!(
            "{} context items deferred; native retrieval retained where available",
            hud.deferred_items
        ));
    }
    parts.push(accounting);
    parts.join(" · ")
}

pub fn usage_fill_percent(report: &ContextUsageReport) -> Option<u32> {
    let limit = report.context_limit_tokens?;
    if limit == 0 {
        return None;
    }
    let pct = (report.total_tokens_estimate as f64 / limit as f64) * 100.0;
    Some(pct.round().clamp(0.0, 100.0) as u32)
}

/// Human-readable multiline breakdown for TUI, WhatsApp, Telegram, etc.
pub fn format_context_usage_text(report: &ContextUsageReport) -> String {
    let mut lines = Vec::new();
    if report.hud.is_some() {
        lines.push(operator_summary(report));
    }
    if let Some(pct) = usage_fill_percent(report) {
        let total = format_tokens(report.total_tokens_estimate);
        let limit = report
            .context_limit_tokens
            .map(format_tokens)
            .unwrap_or_else(|| "?".to_string());
        lines.push(format!("Context {pct}% full · ~{total} / {limit} tokens"));
    } else {
        lines.push(format!(
            "Context ~{} tokens (estimator: {})",
            format_tokens(report.total_tokens_estimate),
            report.estimator
        ));
    }
    lines.push(String::new());
    for layer in &report.layers {
        lines.push(format!(
            "  {} — {}",
            layer.label,
            format_tokens(layer.tokens_estimate)
        ));
    }
    lines.join("\n")
}

fn format_tokens(n: u32) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
