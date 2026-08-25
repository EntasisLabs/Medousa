//! Portable presentation hints applied before a prompt reaches inference.

const MAX_VOICE_APPENDIX_CHARS: usize = 800;

/// Apply the daemon's response-voice metadata to the inference prompt.
///
/// Voice presets describe how the model should answer. They are not an audio
/// capability and must not make an otherwise valid text turn inadmissible.
pub fn append_voice_preset_hint(
    prompt: &str,
    voice_preset_id: Option<&str>,
    voice_appendix: Option<&str>,
) -> String {
    let appendix = voice_appendix
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(appendix) = appendix else {
        return prompt.to_string();
    };
    let preset = voice_preset_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("custom");
    format!(
        "{prompt}\n\n[MEDOUSA_VOICE]\npreset={preset}\n{}",
        truncate_text_for_budget(appendix, MAX_VOICE_APPENDIX_CHARS)
    )
}

fn truncate_text_for_budget(text: &str, max_chars: usize) -> String {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }

    let head = max_chars / 2;
    let tail = max_chars.saturating_sub(head + 5);
    let head_part = text.chars().take(head).collect::<String>();
    let tail_part = text
        .chars()
        .skip(total_chars.saturating_sub(tail))
        .collect::<String>();
    format!("{head_part}\n...\n{tail_part}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_metadata_is_a_prompt_hint_not_an_admission_capability() {
        assert_eq!(
            append_voice_preset_hint("hello", Some("warm"), None),
            "hello"
        );
        assert_eq!(
            append_voice_preset_hint("hello", Some("warm"), Some(" Be direct. ")),
            "hello\n\n[MEDOUSA_VOICE]\npreset=warm\nBe direct."
        );
    }

    #[test]
    fn missing_preset_uses_custom_and_bounds_the_appendix() {
        let rendered = append_voice_preset_hint("hello", None, Some(&"x".repeat(1_000)));
        assert!(rendered.contains("preset=custom"));
        assert!(rendered.chars().count() < 900);
    }
}
