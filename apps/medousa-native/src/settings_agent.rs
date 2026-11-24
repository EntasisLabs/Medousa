//! Medousa Agent: stance, answer depth and the conversation model, kept in
//! the workshop's runtime defaults (`/v1/runtime/tui-defaults`), the same
//! record medousa-home edits. Each change reads the record, sets one field,
//! and writes it back, so fields this app doesn't know survive.
use serde_json::{Value, json};

const PATH: &str = "/v1/runtime/tui-defaults";

/// medousa-home's built-in stances; custom ones come from the record.
pub(crate) const BUILTIN_STANCES: [(&str, &str, &str); 2] = [
    (
        "default",
        "Default",
        "Runtime collaborator voice — no extra stance",
    ),
    (
        "direct",
        "Direct",
        "Action-first — lead with the answer or next move",
    ),
];

/// medousa-home's `DEPTH_CHARTER_OPTIONS`.
pub(crate) const DEPTHS: [(&str, &str, &str); 3] = [
    (
        "concise",
        "Concise",
        "Short answers — less reasoning on the page",
    ),
    ("standard", "Standard", "Balanced depth for everyday work"),
    ("deep", "Deep", "More thorough reasoning and detail"),
];

/// One change to the record.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Patch {
    Stance(String),
    Depth(String),
    /// The main profile's provider and model, mirrored into the flat fields.
    Model(String, String),
    /// One top-level field, as Runtime Controls sets them.
    Field(&'static str, Value),
    /// The conversation model's fallback at an index: set, or cleared.
    Fallback(usize, Option<(String, String)>),
}

/// The conversation model's fallbacks, in order.
pub(crate) fn fallbacks(defaults: &Value) -> Vec<(String, String)> {
    defaults["inference_profiles"]["main"]["fallbacks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            Some((
                text(&f["provider"])?.to_string(),
                text(&f["model"])?.to_string(),
            ))
        })
        .collect()
}

/// medousa-home offers two fallback slots.
pub(crate) const FALLBACK_SLOTS: [&str; 2] = ["fallback.0", "fallback.1"];

/// medousa-home's `CODING_RUNTIMES`: Medousa Coder, then the external
/// agents.
pub(crate) fn coding_runtimes() -> Vec<(&'static str, &'static str)> {
    std::iter::once(("medousa", "Medousa Coder"))
        .chain(super::external::AGENTS.iter().map(|a| (a.runtime, a.label)))
        .collect()
}

/// medousa-home's `normalizeCodingRuntimePreferences`.
pub(crate) fn coding(defaults: &Value) -> (String, Vec<String>) {
    let all = coding_runtimes();
    let valid = |v: &str| all.iter().any(|(id, _)| *id == v);
    let raw = &defaults["coding_runtime"];
    let preferred = text(&raw["preferred"])
        .filter(|p| valid(p))
        .unwrap_or("medousa")
        .to_string();
    let mut fallbacks: Vec<String> = Vec::new();
    for f in raw["fallbacks"].as_array().into_iter().flatten() {
        if let Some(f) = f.as_str()
            && valid(f)
            && f != preferred
            && !fallbacks.iter().any(|x| x == f)
        {
            fallbacks.push(f.to_string());
        }
    }
    (preferred, fallbacks)
}

pub(crate) fn coding_value(preferred: &str, fallbacks: &[String]) -> Value {
    json!({ "preferred": preferred, "fallbacks": fallbacks })
}

/// The stages of a turn, with medousa-home's names.
pub(crate) const STAGES: [(&str, &str); 7] = [
    ("orchestrator", "Lead"),
    ("chunker", "Reader"),
    ("extractor", "Extractor"),
    ("summarizer", "Summarizer"),
    ("verifier", "Verifier"),
    ("packer", "Packer"),
    ("final_response", "Final voice"),
];

/// One stage's provider, model and fallback chain.
pub(crate) fn stage(defaults: &Value, key: &str) -> Option<(String, String, Vec<String>)> {
    let r = &defaults["stage_routing"][key];
    r.is_object().then(|| {
        (
            text(&r["provider"]).unwrap_or_default().to_string(),
            text(&r["model"]).unwrap_or_default().to_string(),
            r["fallback_chain"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
        )
    })
}

/// The routing matrix with one stage changed, keeping the rest of it.
pub(crate) fn with_stage(
    defaults: &Value,
    key: &str,
    provider: &str,
    model: &str,
    chain: &[String],
) -> Option<Value> {
    let mut matrix = defaults["stage_routing"].clone();
    let route = matrix.get_mut(key).filter(|r| r.is_object())?;
    route["provider"] = json!(provider.trim());
    route["model"] = json!(model.trim());
    route["fallback_chain"] = json!(chain);
    Some(matrix)
}

/// A stance as offered: id, name, description.
pub(crate) fn stances(defaults: &Value) -> Vec<(String, String, String)> {
    let mut out: Vec<_> = BUILTIN_STANCES
        .iter()
        .map(|(id, name, hint)| (id.to_string(), name.to_string(), hint.to_string()))
        .collect();
    for preset in defaults["custom_voice_presets"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let (Some(id), Some(name)) = (preset["id"].as_str(), preset["name"].as_str()) else {
            continue;
        };
        if out.iter().any(|(existing, ..)| existing == id) {
            continue;
        }
        out.push((
            id.to_string(),
            name.to_string(),
            preset["description"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        ));
    }
    out
}

fn text(v: &Value) -> Option<&str> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty())
}

pub(crate) fn stance(defaults: &Value) -> String {
    text(&defaults["active_voice_id"])
        .unwrap_or("default")
        .to_string()
}

pub(crate) fn depth(defaults: &Value) -> String {
    text(&defaults["response_depth_mode"])
        .unwrap_or("standard")
        .to_string()
}

/// A role's provider and model: its profile, else (for main) the flat fields.
pub(crate) fn role(defaults: &Value, key: &str) -> Option<(String, String)> {
    let profile = &defaults["inference_profiles"][key];
    match (text(&profile["provider"]), text(&profile["model"])) {
        (Some(p), Some(m)) => Some((p.to_string(), m.to_string())),
        _ if key == "main" => match (text(&defaults["provider"]), text(&defaults["model"])) {
            (Some(p), Some(m)) => Some((p.to_string(), m.to_string())),
            _ => None,
        },
        _ => None,
    }
}

/// Applies `patch` to the record in place.
pub(crate) fn apply(defaults: &mut Value, patch: &Patch) {
    if !defaults.is_object() {
        *defaults = json!({});
    }
    match patch {
        Patch::Stance(id) => defaults["active_voice_id"] = json!(id),
        Patch::Depth(mode) => defaults["response_depth_mode"] = json!(mode),
        Patch::Field(key, value) => defaults[*key] = value.clone(),
        Patch::Fallback(index, target) => {
            if !defaults["inference_profiles"].is_object() {
                defaults["inference_profiles"] = json!({});
            }
            if !defaults["inference_profiles"]["main"].is_object() {
                defaults["inference_profiles"]["main"] = json!({
                    "provider": defaults["provider"].clone(),
                    "model": defaults["model"].clone(),
                });
            }
            let main = &mut defaults["inference_profiles"]["main"];
            if !main["fallbacks"].is_array() {
                main["fallbacks"] = json!([]);
            }
            let list = main["fallbacks"].as_array_mut().unwrap();
            match target {
                Some((provider, model)) => {
                    let entry = json!({ "provider": provider, "model": model });
                    if *index < list.len() {
                        list[*index] = entry;
                    } else {
                        list.push(entry);
                    }
                }
                None if *index < list.len() => {
                    list.remove(*index);
                }
                None => {}
            }
        }
        Patch::Model(provider, model) => {
            let changed = role(defaults, "main").is_none_or(|(p, _)| p != *provider);
            if !defaults["inference_profiles"].is_object() {
                defaults["inference_profiles"] = json!({});
            }
            if !defaults["inference_profiles"]["main"].is_object() {
                defaults["inference_profiles"]["main"] = json!({ "fallbacks": [] });
            }
            let main = &mut defaults["inference_profiles"]["main"];
            main["provider"] = json!(provider);
            main["model"] = json!(model);
            // A base URL belongs to the provider it was set for.
            if changed && let Some(main) = main.as_object_mut() {
                main.remove("baseUrl");
            }
            defaults["provider"] = json!(provider);
            defaults["model"] = json!(model);
            if changed && let Some(flat) = defaults.as_object_mut() {
                flat.remove("base_url");
            }
        }
    }
}

pub(crate) async fn read(client: &medousa_sdk::MedousaClient) -> Result<Value, String> {
    client.http().get(PATH).await.map_err(|e| e.to_string())
}

/// Reads the record, applies `patch`, writes it back, and returns what was
/// written.
pub(crate) async fn write(
    client: &medousa_sdk::MedousaClient,
    patch: &Patch,
) -> Result<Value, String> {
    let mut defaults = read(client).await?;
    apply(&mut defaults, patch);
    let _: Value = client
        .http()
        .put(PATH, &defaults)
        .await
        .map_err(|e| e.to_string())?;
    Ok(defaults)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallbacks_coding_and_stages_change_one_piece_at_a_time() {
        let mut d = json!({"provider": "anthropic", "model": "a"});
        apply(
            &mut d,
            &Patch::Fallback(0, Some(("openai".into(), "b".into()))),
        );
        apply(
            &mut d,
            &Patch::Fallback(5, Some(("groq".into(), "c".into()))),
        );
        assert_eq!(
            fallbacks(&d),
            [("openai".into(), "b".into()), ("groq".into(), "c".into())]
        );
        assert_eq!(d["inference_profiles"]["main"]["provider"], "anthropic");
        apply(&mut d, &Patch::Fallback(0, None));
        assert_eq!(fallbacks(&d), [("groq".to_string(), "c".to_string())]);

        let raw = json!({"coding_runtime": {"preferred": "codex", "fallbacks": ["codex", "bogus", "cursor", "cursor"]}});
        assert_eq!(coding(&raw), ("codex".into(), vec!["cursor".to_string()]));
        assert_eq!(coding(&json!({})).0, "medousa");

        let route = |p: &str| json!({"role": p, "provider": "x", "model": "y", "policy_profile": "default", "fallback_chain": []});
        let d = json!({"stage_routing": {"orchestrator": route("orchestrator"), "verifier": route("verifier")}});
        let m = with_stage(&d, "verifier", " openai ", "b", &["groq:c".into()]).unwrap();
        assert_eq!(m["verifier"]["provider"], "openai");
        assert_eq!(m["verifier"]["policy_profile"], "default", "kept");
        assert_eq!(m["orchestrator"], route("orchestrator"));
        assert!(with_stage(&json!({}), "verifier", "a", "b", &[]).is_none());
    }

    #[test]
    fn changes_touch_only_their_field() {
        let mut d = json!({
            "active_voice_id": "default",
            "slice_hot_window_turns": 12,
            "custom_voice_presets": [{"id": "warm", "name": "Warm", "voiceAppendix": "…"}],
            "inference_profiles": {"main": {"provider": "anthropic", "model": "a", "baseUrl": "x", "fallbacks": [1]}},
            "provider": "anthropic",
            "model": "a",
            "base_url": "x"
        });
        apply(&mut d, &Patch::Stance("warm".into()));
        apply(&mut d, &Patch::Depth("deep".into()));
        assert_eq!(stance(&d), "warm");
        assert_eq!(depth(&d), "deep");
        assert_eq!(d["slice_hot_window_turns"], 12, "unknown fields survive");
        assert_eq!(stances(&d).len(), 3);
        // Same provider: the base URL stays; another provider drops it.
        apply(&mut d, &Patch::Model("anthropic".into(), "b".into()));
        assert_eq!(d["inference_profiles"]["main"]["baseUrl"], "x");
        assert_eq!(d["inference_profiles"]["main"]["fallbacks"], json!([1]));
        apply(&mut d, &Patch::Model("openai".into(), "c".into()));
        assert_eq!(role(&d, "main"), Some(("openai".into(), "c".into())));
        assert!(d["inference_profiles"]["main"].get("baseUrl").is_none());
        assert!(d.get("base_url").is_none());
        assert_eq!(
            (d["provider"].as_str(), d["model"].as_str()),
            (Some("openai"), Some("c"))
        );
    }

    #[test]
    fn an_empty_record_reads_as_medousa_home_defaults() {
        let d = json!({});
        assert_eq!(stance(&d), "default");
        assert_eq!(depth(&d), "standard");
        assert_eq!(role(&d, "main"), None);
        let mut d = Value::Null;
        apply(&mut d, &Patch::Model("ollama".into(), "llama3".into()));
        assert_eq!(role(&d, "main"), Some(("ollama".into(), "llama3".into())));
    }
}
