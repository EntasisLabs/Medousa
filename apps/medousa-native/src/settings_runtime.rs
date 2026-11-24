//! Runtime Controls: how she reaches for tools, the shell sandbox, and the
//! engine, kept in the same runtime defaults record as Medousa Agent. Each
//! control sets one top-level field; the record keeps the rest.
use serde_json::{Value, json};

/// A choice: the stored id, its name, and what it means.
pub(crate) type Choice = (&'static str, &'static str, &'static str);

/// medousa-home's `TOOL_CALL_CHARTER_OPTIONS`.
pub(crate) const POSTURES: [Choice; 2] = [
    (
        "auto",
        "Flexible",
        "She decides when tools are worth calling",
    ),
    ("strict", "Careful", "Tighter rules before invoking tools"),
];

/// medousa-home's `HOST_BUS_CHARTER_OPTIONS`.
pub(crate) const SPECIALISTS: [Choice; 3] = [
    (
        "auto",
        "When needed",
        "Bring specialists in only when the turn needs help",
    ),
    (
        "force",
        "Always",
        "Route through the specialist bus every turn",
    ),
    ("off", "Direct", "Orchestrator only — no specialist bus"),
];

/// medousa-home's `WEB_SEARCH_PROVIDER_OPTIONS`; empty means capability order.
pub(crate) const SEARCH: [Choice; 4] = [
    ("", "Auto (capability order)", ""),
    ("duckduckgo", "DuckDuckGo", ""),
    ("google", "Google", ""),
    ("tavily", "Tavily", ""),
];

/// medousa-home's `BACKEND_OPTIONS` with its labels.
pub(crate) const BACKENDS: [Choice; 3] = [
    (
        "surreal-mem",
        "Surreal (memory)",
        "Default — fast in-process store",
    ),
    (
        "in-memory",
        "In-memory",
        "Ephemeral — cleared when the engine stops",
    ),
    ("surreal-kv", "Surreal (disk)", "Persists across restarts"),
];

/// A picker: its record field, its default, and its choices.
pub(crate) struct Pick {
    pub key: &'static str,
    pub default: &'static str,
    pub choices: &'static [Choice],
}

pub(crate) const POSTURE: Pick = Pick {
    key: "tool_call_mode",
    default: "auto",
    choices: &POSTURES,
};
pub(crate) const SPECIALIST: Pick = Pick {
    key: "host_turn_bus_mode",
    default: "auto",
    choices: &SPECIALISTS,
};
pub(crate) const WEB_SEARCH: Pick = Pick {
    key: "web_search_preferred_provider",
    default: "",
    choices: &SEARCH,
};
pub(crate) const BACKEND: Pick = Pick {
    key: "backend",
    default: "surreal-mem",
    choices: &BACKENDS,
};

impl Pick {
    /// The chosen entry; an unknown id reads as the default.
    pub(crate) fn chosen(&self, d: &Value) -> &'static Choice {
        let id = d[self.key].as_str().map(str::trim).unwrap_or(self.default);
        self.choices
            .iter()
            .find(|(c, ..)| *c == id)
            .or_else(|| self.choices.iter().find(|(c, ..)| *c == self.default))
            .unwrap_or(&self.choices[0])
    }
}

/// An on/off field.
pub(crate) struct Flag {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub default: bool,
}

impl Flag {
    pub(crate) fn on(&self, d: &Value) -> bool {
        d[self.key].as_bool().unwrap_or(self.default)
    }
}

pub(crate) const SEARCH_FALLBACKS: Flag = Flag {
    key: "web_search_try_fallbacks",
    label: "Search fallbacks",
    hint: "Try others when preferred fails",
    default: true,
};
pub(crate) const SHELL_TOOLS: Flag = Flag {
    key: "shell_agent_tools_enabled",
    label: "Agent shell tools",
    hint: "Unlock cognition_shell_* for specialists",
    default: false,
};
pub(crate) const SHELL_NETWORK: Flag = Flag {
    key: "shell_network_default",
    label: "Network ceiling",
    hint: "Calls may opt in only if this is on",
    default: false,
};
pub(crate) const THINKING: Flag = Flag {
    key: "thinking_capture",
    label: "Thinking traces",
    hint: "Capture reasoning lines for debugging",
    default: true,
};
pub(crate) const TELEMETRY: Flag = Flag {
    key: "stasis_otel_enabled",
    label: "Telemetry export",
    hint: "OpenTelemetry from Stasis",
    default: false,
};

/// How a number is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Show {
    Count(&'static str),
    /// Milliseconds, shown in seconds.
    Seconds,
    /// Bytes, shown in KB.
    Kilobytes,
    /// A 0–1 ratio.
    Ratio,
}

/// A number field, stepped by − and +.
pub(crate) struct Num {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub show: Show,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub default: f64,
}

const fn rounds(
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    min: f64,
    max: f64,
    default: f64,
) -> Num {
    Num {
        key,
        label,
        hint,
        show: Show::Count("rounds"),
        min,
        max,
        step: 1.,
        default,
    }
}

const fn ratio(key: &'static str, label: &'static str, hint: &'static str, default: f64) -> Num {
    Num {
        key,
        label,
        hint,
        show: Show::Ratio,
        min: 0.,
        max: 1.,
        step: 0.05,
        default,
    }
}

pub(crate) const TOOL_ROUNDS: Num = rounds(
    "max_tool_rounds",
    "General tool rounds",
    "Coder uses a separate 100-round ceiling",
    1.,
    48.,
    30.,
);
pub(crate) const SHELL_TIMEOUT: Num = Num {
    key: "shell_timeout_ms",
    label: "Timeout",
    hint: "Hard stop for one process",
    show: Show::Seconds,
    min: 1_000.,
    max: 600_000.,
    step: 5_000.,
    default: 30_000.,
};
pub(crate) const SHELL_OUTPUT: Num = Num {
    key: "shell_max_output_bytes",
    label: "Max output",
    hint: "Stdout/stderr before truncation",
    show: Show::Kilobytes,
    min: 1_024.,
    max: 16_777_216.,
    step: 65_536.,
    default: 262_144.,
};
pub(crate) const THINKING_LINES: Num = Num {
    key: "thinking_max_lines",
    label: "Thinking max lines",
    hint: "Cap retained per turn",
    show: Show::Count("lines"),
    min: 50.,
    max: 2_000.,
    step: 50.,
    default: 300.,
};

/// medousa-home's `toolBudgetFields`.
pub(crate) const BUDGETS: [Num; 6] = [
    rounds(
        "host_bus_max_tool_rounds",
        "Specialist rounds",
        "Tool calls helpers may make in one turn",
        1.,
        48.,
        8.,
    ),
    rounds(
        "activation_tool_intent_max_rounds",
        "Heavy-turn budget",
        "Extra room when a turn clearly needs tools",
        1.,
        48.,
        4.,
    ),
    rounds(
        "activation_short_turn_max_tool_rounds",
        "Short-turn budget",
        "Fewer calls when the turn looks light",
        1.,
        24.,
        6.,
    ),
    rounds(
        "continuation_max_tool_rounds",
        "Follow-up budget",
        "Tool calls on continuation turns",
        1.,
        48.,
        8.,
    ),
    Num {
        show: Show::Count("tries"),
        ..rounds(
            "max_text_only_stuck_continues",
            "Stuck-turn retries",
            "Nudges for a stuck text-only turn",
            0.,
            12.,
            2.,
        )
    },
    rounds(
        "classifier_restricted_max_tool_rounds",
        "Restricted budget",
        "When reach is tightened for safety",
        0.,
        24.,
        4.,
    ),
];

/// medousa-home's retry and verifier fields.
pub(crate) const QUALITY: [Num; 6] = [
    Num {
        show: Show::Count("tries"),
        ..rounds(
            "retry_runtime_max_retries",
            "Max retries",
            "Retries after a failed step",
            0.,
            12.,
            1.,
        )
    },
    rounds(
        "retry_runtime_max_rounds",
        "Retry rounds",
        "Round budget while retrying",
        0.,
        48.,
        12.,
    ),
    ratio(
        "verifier_min_citation_coverage",
        "Citation coverage",
        "Minimum share of claims that need citations",
        0.6,
    ),
    ratio(
        "verifier_min_avg_support_strength",
        "Avg support",
        "Minimum average support strength",
        0.7,
    ),
    ratio(
        "verifier_min_supported_claim_ratio",
        "Supported claims",
        "Minimum ratio of claims that pass support",
        0.6,
    ),
    ratio(
        "verifier_min_claim_support_strength",
        "Claim support",
        "Minimum support for one claim",
        0.65,
    ),
];

impl Num {
    pub(crate) fn value(&self, d: &Value) -> f64 {
        d[self.key].as_f64().unwrap_or(self.default)
    }

    /// The value one step down (`-1`) or up (`1`), kept in range; `None` at
    /// the edge.
    pub(crate) fn stepped(&self, d: &Value, dir: i8) -> Option<Value> {
        let now = self.value(d);
        // Off-grid values snap to the grid before stepping.
        let steps = ((now - self.min) / self.step).round() + f64::from(dir);
        let next = (self.min + steps * self.step).clamp(self.min, self.max);
        if (next - now).abs() < 1e-9 {
            return None;
        }
        Some(if self.show == Show::Ratio {
            json!((next * 100.).round() / 100.)
        } else {
            json!(next.round() as u64)
        })
    }

    pub(crate) fn shown(&self, d: &Value) -> String {
        let v = self.value(d);
        match self.show {
            Show::Count(unit) => format!("{} {unit}", v.round() as u64),
            Show::Seconds => {
                let s = v / 1000.;
                if s.fract() == 0. {
                    format!("{s} s")
                } else {
                    format!("{s:.1} s")
                }
            }
            Show::Kilobytes => format!("{} KB", (v / 1024.).round() as u64),
            Show::Ratio => format!("{v:.2}"),
        }
    }
}

/// The section's one-line summary, as medousa-home builds it.
pub(crate) fn summary(d: &Value) -> String {
    format!(
        "{} · {} · {} · {} · {} rounds",
        POSTURE.chosen(d).1,
        SPECIALIST.chosen(d).1,
        if SHELL_TOOLS.on(d) {
            "Shell on"
        } else {
            "Shell off"
        },
        if d["vault_git_enabled"].as_bool().unwrap_or(false) {
            "Versions on"
        } else {
            "Versions off"
        },
        TOOL_ROUNDS.value(d).round() as u64,
    )
}

/// Shell tools are on with nothing listed: any command basename can run.
pub(crate) fn shell_unguarded(d: &Value) -> bool {
    SHELL_TOOLS.on(d)
        && d["shell_allowed_binaries"].as_array().is_none_or(|a| {
            a.iter()
                .all(|b| b.as_str().is_none_or(|s| s.trim().is_empty()))
        })
}

/// A list field's entries, for showing.
pub(crate) fn list(d: &Value, key: &str) -> Vec<String> {
    d[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::trim))
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// `/v1/runtime/workers`: queue concurrency and preferred lane shares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Workers {
    pub max_in_flight: usize,
    pub agents: usize,
    pub scheduled: usize,
    pub delivery: usize,
    pub maintenance: usize,
}

/// The lanes: field, name, hint.
pub(crate) const LANES: [(&str, &str, &str); 4] = [
    ("agents", "Agents", "Delegated and long-running LLM work"),
    (
        "scheduled",
        "Scheduled",
        "Recurring and general queued jobs",
    ),
    ("delivery", "Delivery", "Outbox and channel publication"),
    (
        "maintenance",
        "Maintenance",
        "Retention and background upkeep",
    ),
];

impl Workers {
    pub(crate) fn get(&self, field: &str) -> usize {
        match field {
            "max_in_flight" => self.max_in_flight,
            "agents" => self.agents,
            "scheduled" => self.scheduled,
            "delivery" => self.delivery,
            _ => self.maintenance,
        }
    }

    /// One field stepped by `delta`, kept at 1 or more for the total and 0
    /// or more for a lane.
    pub(crate) fn stepped(mut self, field: &str, delta: i64) -> Self {
        let min = usize::from(field == "max_in_flight");
        let next = (self.get(field) as i64 + delta).max(min as i64) as usize;
        match field {
            "max_in_flight" => self.max_in_flight = next,
            "agents" => self.agents = next,
            "scheduled" => self.scheduled = next,
            "delivery" => self.delivery = next,
            _ => self.maintenance = next,
        }
        self
    }

    pub(crate) fn reserved(&self) -> usize {
        self.agents + self.scheduled + self.delivery + self.maintenance
    }

    /// medousa-home's check before saving.
    pub(crate) fn problem(&self) -> Option<String> {
        (self.reserved() > self.max_in_flight).then(|| {
            format!(
                "Lane shares total {}, which exceeds global capacity {}.",
                self.reserved(),
                self.max_in_flight
            )
        })
    }
}

/// The next preset from `value` in direction `dir` (−1 or 1), snapping an
/// off-grid value to its neighbour; `None` at either end.
pub(crate) fn preset(steps: &[i64], value: i64, dir: i64) -> Option<i64> {
    if dir < 0 {
        steps.iter().rev().find(|&&v| v < value).copied()
    } else {
        steps.iter().find(|&&v| v > value).copied()
    }
}

/// medousa-home's `parseMultilineList`: commas or new lines.
pub(crate) fn split_list(raw: &str) -> Vec<String> {
    raw.split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// The text fields a disclosure owns, dropped when it closes.
pub(crate) fn drafts_for(disclosure: &str) -> &'static [&'static str] {
    match disclosure {
        "runtime.tools" => &["rt.modules"],
        "runtime.allowlists" => &["rt.binaries", "rt.roots"],
        "runtime.env" => &["rt.env.name", "rt.env.value"],
        _ => &[],
    }
}

/// The names set in `env_overrides` (`KEY=value` per line), in order.
pub(crate) fn env_names(env: &str) -> Vec<String> {
    env.lines()
        .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim()))
        .filter(|k| !k.is_empty())
        .map(String::from)
        .collect()
}

fn names(line: &str, name: &str) -> bool {
    line.split_once('=').is_some_and(|(k, _)| k.trim() == name)
}

/// Sets one variable, replacing its line or adding one; other lines stay.
pub(crate) fn env_set(env: &str, name: &str, value: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['=', ' ', '\t']) {
        return Err("A variable name has no spaces or “=”, like API_TIMEOUT.".into());
    }
    if value.contains('\n') {
        return Err("A value fits on one line.".into());
    }
    let line = format!("{name}={value}");
    let mut lines: Vec<String> = Vec::new();
    let mut replaced = false;
    for l in env.lines() {
        if names(l, name) {
            if !replaced {
                lines.push(line.clone());
                replaced = true;
            }
        } else {
            lines.push(l.to_string());
        }
    }
    if !replaced {
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

/// Drops a variable's lines; other lines stay.
pub(crate) fn env_remove(env: &str, name: &str) -> String {
    env.lines()
        .filter(|l| !names(l, name))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_record_reads_as_medousa_home_defaults() {
        let d = json!({});
        assert_eq!(
            summary(&d),
            "Flexible · When needed · Shell off · Versions off · 30 rounds"
        );
        assert_eq!(BACKEND.chosen(&d).1, "Surreal (memory)");
        assert_eq!(WEB_SEARCH.chosen(&d).1, "Auto (capability order)");
        assert!(SEARCH_FALLBACKS.on(&d));
        assert_eq!(SHELL_TIMEOUT.shown(&d), "30 s");
        assert_eq!(SHELL_OUTPUT.shown(&d), "256 KB");
        assert_eq!(QUALITY[2].shown(&d), "0.60");
        assert!(!shell_unguarded(&d));
        assert!(shell_unguarded(&json!({"shell_agent_tools_enabled": true})));
        assert!(!shell_unguarded(
            &json!({"shell_agent_tools_enabled": true, "shell_allowed_binaries": ["git"]})
        ));
    }

    #[test]
    fn steps_stay_in_range_and_on_the_grid() {
        let d = json!({ "max_tool_rounds": 48, "verifier_min_citation_coverage": 0.62 });
        assert_eq!(TOOL_ROUNDS.stepped(&d, 1), None, "at the top");
        assert_eq!(TOOL_ROUNDS.stepped(&d, -1), Some(json!(47)));
        assert_eq!(QUALITY[2].stepped(&d, 1), Some(json!(0.65)));
        assert_eq!(QUALITY[2].stepped(&d, -1), Some(json!(0.55)));
        let d = json!({ "shell_timeout_ms": 1000 });
        assert_eq!(SHELL_TIMEOUT.stepped(&d, -1), None);
        assert_eq!(SHELL_TIMEOUT.stepped(&d, 1), Some(json!(6000)));
        assert_eq!(
            POSTURE.chosen(&json!({"tool_call_mode": "bogus"})).1,
            "Flexible"
        );
    }

    #[test]
    fn lists_split_like_medousa_home_and_env_lines_change_one_at_a_time() {
        assert_eq!(split_list("git, ls\nrg,,"), ["git", "ls", "rg"]);
        let env = "# keep\nA=1\nB=two=2";
        assert_eq!(env_names(env), ["A", "B"]);
        assert_eq!(env_set(env, "A", "9").unwrap(), "# keep\nA=9\nB=two=2");
        assert_eq!(
            env_set(env, " C ", "x").unwrap(),
            "# keep\nA=1\nB=two=2\nC=x"
        );
        assert!(env_set(env, "BAD NAME", "x").is_err());
        assert!(env_set(env, "", "x").is_err());
        assert_eq!(env_remove(env, "A"), "# keep\nB=two=2");
        assert_eq!(drafts_for("runtime.env"), ["rt.env.name", "rt.env.value"]);
    }

    #[test]
    fn presets_step_and_snap() {
        let s = [5, 15, 30];
        assert_eq!(preset(&s, 15, 1), Some(30));
        assert_eq!(preset(&s, 15, -1), Some(5));
        assert_eq!(preset(&s, 20, -1), Some(15));
        assert_eq!(preset(&s, 30, 1), None);
        assert_eq!(preset(&s, 5, -1), None);
    }

    #[test]
    fn worker_shares_step_within_bounds_and_must_fit() {
        let w = Workers {
            max_in_flight: 2,
            agents: 1,
            scheduled: 0,
            delivery: 0,
            maintenance: 1,
        };
        assert_eq!(w.stepped("scheduled", -1).scheduled, 0);
        assert_eq!(w.stepped("max_in_flight", -5).max_in_flight, 1);
        assert!(w.problem().is_none());
        let over = w.stepped("agents", 1);
        assert_eq!(
            over.problem().as_deref(),
            Some("Lane shares total 3, which exceeds global capacity 2.")
        );
    }
}
