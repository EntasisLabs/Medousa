//! Explicit main / vision / STT inference profiles (Phase 2).


use crate::session::TuiDefaults;

pub use medousa_types::inference::{InferenceProfile, InferenceProfilesConfig, InferenceTarget};

pub const MAX_FALLBACKS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceProfileKind {
    Main,
    Vision,
    Stt,
}

pub fn normalize_tui_defaults(defaults: &mut TuiDefaults) {
    if defaults.inference_profiles.is_none() {
        defaults.inference_profiles = Some(InferenceProfilesConfig::default());
    }
    let profiles = defaults.inference_profiles.get_or_insert_with(Default::default);

    if profiles.main.as_ref().and_then(|profile| profile.trimmed()).is_none() {
        profiles.main = Some(InferenceProfile {
            provider: crate::resolve_llm_provider(defaults.provider.as_deref()),
            model: crate::resolve_llm_model(defaults.model.as_deref()),
            base_url: defaults
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            fallbacks: Vec::new(),
        });
    }

    if profiles.stt.as_ref().and_then(|profile| profile.trimmed()).is_none() {
        let provider = defaults
            .stt_provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "openai".to_string());
        let model = defaults
            .stt_model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_stt_model(&provider));
        profiles.stt = Some(InferenceProfile {
            provider,
            model,
            base_url: defaults
                .stt_base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            fallbacks: Vec::new(),
        });
    }

    if let Some(main) = profiles.main.as_mut() {
        if let Some(trimmed) = main.trimmed() {
            *main = trimmed;
        }
    }
    if let Some(vision) = profiles.vision.as_mut() {
        if let Some(trimmed) = vision.trimmed() {
            *vision = trimmed;
        } else {
            profiles.vision = None;
        }
    }
    if let Some(stt) = profiles.stt.as_mut() {
        if let Some(trimmed) = stt.trimmed() {
            *stt = trimmed;
        }
    }

    sync_flat_fields_from_profiles(defaults);
}

pub fn sync_top_level_from_main(defaults: &mut TuiDefaults) {
    normalize_tui_defaults(defaults);
    let Some(main) = defaults
        .inference_profiles
        .as_ref()
        .and_then(|profiles| profiles.main.as_ref())
    else {
        return;
    };
    defaults.provider = Some(main.provider.clone());
    defaults.model = Some(main.model.clone());
    defaults.base_url = main.base_url.clone();
}

pub fn sync_flat_fields_from_profiles(defaults: &mut TuiDefaults) {
    let Some(profiles) = defaults.inference_profiles.as_ref() else {
        return;
    };
    if let Some(main) = profiles.main.as_ref() {
        defaults.provider = Some(main.provider.clone());
        defaults.model = Some(main.model.clone());
        defaults.base_url = main.base_url.clone();
    }
    if let Some(stt) = profiles.stt.as_ref() {
        defaults.stt_provider = Some(stt.provider.clone());
        defaults.stt_model = Some(stt.model.clone());
        defaults.stt_base_url = stt.base_url.clone();
    }
}

pub fn main_target(defaults: &TuiDefaults) -> InferenceTarget {
    if let Some(profile) = defaults
        .inference_profiles
        .as_ref()
        .and_then(|profiles| profiles.main.as_ref())
        .and_then(|profile| profile.trimmed())
    {
        return profile.as_target();
    }
    InferenceTarget {
        provider: crate::resolve_llm_provider(defaults.provider.as_deref()),
        model: crate::resolve_llm_model(defaults.model.as_deref()),
        base_url: defaults
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }
}

pub fn vision_target(defaults: &TuiDefaults) -> Option<InferenceTarget> {
    defaults
        .inference_profiles
        .as_ref()
        .and_then(|profiles| profiles.vision.as_ref())
        .and_then(|profile| profile.trimmed())
        .map(|profile| profile.as_target())
}

pub fn stt_target(defaults: &TuiDefaults) -> Option<InferenceTarget> {
    defaults
        .inference_profiles
        .as_ref()
        .and_then(|profiles| profiles.stt.as_ref())
        .and_then(|profile| profile.trimmed())
        .map(|profile| profile.as_target())
        .or_else(|| {
            let provider = defaults.stt_provider.as_deref()?.trim();
            let model = defaults.stt_model.as_deref()?.trim();
            if provider.is_empty() || model.is_empty() {
                return None;
            }
            Some(InferenceTarget {
                provider: provider.to_string(),
                model: model.to_string(),
                base_url: defaults
                    .stt_base_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
            })
        })
}

pub fn vision_profile_ready(defaults: &TuiDefaults) -> bool {
    vision_target(defaults).is_some()
}

pub fn format_profile_line(label: &str, profile: Option<&InferenceProfile>) -> String {
    let Some(profile) = profile.and_then(|profile| profile.trimmed()) else {
        return format!("{label}: (not configured — edit in Medousa Home Settings → Models)");
    };
    let target = profile.as_target();
    let mut line = format!("{label}: {}:{}", target.provider, target.model);
    if !profile.fallbacks.is_empty() {
        let count = profile.fallbacks.len();
        line.push_str(&format!(
            " (+{count} fallback{})",
            if count == 1 { "" } else { "s" }
        ));
    }
    line
}

pub fn profile_lines_from_defaults(defaults: &TuiDefaults) -> (String, String, String) {
    let profiles = defaults.inference_profiles.as_ref();
    let main_line = profiles
        .and_then(|profiles| profiles.main.as_ref())
        .map(|profile| format_profile_line("Main chat profile", Some(profile)))
        .unwrap_or_else(|| {
            format!(
                "Main chat profile: {}:{}",
                crate::resolve_llm_provider(defaults.provider.as_deref()),
                crate::resolve_llm_model(defaults.model.as_deref())
            )
        });
    let vision_line = format_profile_line(
        "Vision profile",
        profiles.and_then(|profiles| profiles.vision.as_ref()),
    );
    let stt_line = profiles
        .and_then(|profiles| profiles.stt.as_ref())
        .map(|profile| format_profile_line("Speech input profile", Some(profile)))
        .unwrap_or_else(|| {
            let provider = defaults
                .stt_provider
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(provider) = provider else {
                return format_profile_line("Speech input profile", None);
            };
            let model = defaults
                .stt_model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| default_stt_model(provider));
            format!("Speech input profile: {provider}:{model}")
        });
    (main_line, vision_line, stt_line)
}

pub fn validate_profiles(profiles: &InferenceProfilesConfig) -> Result<(), String> {
    let main = profiles
        .main
        .as_ref()
        .and_then(|profile| profile.trimmed())
        .ok_or_else(|| "main profile requires provider and model".to_string())?;
    let stt = profiles
        .stt
        .as_ref()
        .and_then(|profile| profile.trimmed())
        .ok_or_else(|| "stt profile requires provider and model".to_string())?;

    for (label, profile) in [("main", &main), ("stt", &stt)] {
        validate_fallbacks(label, &profile.fallbacks)?;
    }
    if let Some(vision) = profiles.vision.as_ref().and_then(|profile| profile.trimmed()) {
        validate_fallbacks("vision", &vision.fallbacks)?;
    }
    Ok(())
}

fn validate_fallbacks(label: &str, fallbacks: &[InferenceTarget]) -> Result<(), String> {
    if fallbacks.len() > MAX_FALLBACKS {
        return Err(format!(
            "{label} profile supports at most {MAX_FALLBACKS} fallbacks"
        ));
    }
    for (index, fallback) in fallbacks.iter().enumerate() {
        if fallback.trimmed().is_none() {
            return Err(format!(
                "{label} fallback {} requires provider and model",
                index + 1
            ));
        }
    }
    Ok(())
}

pub fn default_stt_model(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "groq" => "whisper-large-v3".to_string(),
        _ => "whisper-1".to_string(),
    }
}

pub fn apply_profiles(defaults: &mut TuiDefaults, profiles: InferenceProfilesConfig) -> Result<(), String> {
    validate_profiles(&profiles)?;
    defaults.inference_profiles = Some(profiles);
    normalize_tui_defaults(defaults);
    sync_top_level_from_main(defaults);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_flat_fields_into_profiles_on_normalize() {
        let mut defaults = TuiDefaults {
            provider: Some("openai".to_string()),
            model: Some("gpt-4o-mini".to_string()),
            stt_provider: Some("groq".to_string()),
            stt_model: Some("whisper-large-v3".to_string()),
            ..TuiDefaults::default()
        };
        normalize_tui_defaults(&mut defaults);
        let profiles = defaults.inference_profiles.expect("profiles");
        assert_eq!(
            profiles.main.as_ref().map(|profile| profile.model.as_str()),
            Some("gpt-4o-mini")
        );
        assert_eq!(
            profiles.stt.as_ref().map(|profile| profile.provider.as_str()),
            Some("groq")
        );
    }

    #[test]
    fn vision_profile_optional_until_operator_sets_it() {
        let mut defaults = TuiDefaults::default();
        normalize_tui_defaults(&mut defaults);
        assert!(!vision_profile_ready(&defaults));
        defaults.inference_profiles.as_mut().unwrap().vision = Some(InferenceProfile {
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: None,
            fallbacks: Vec::new(),
        });
        assert!(vision_profile_ready(&defaults));
    }
}
