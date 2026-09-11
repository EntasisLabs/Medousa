//! Validate a bounded batch against one immutable UTF-8 file revision.

use schemars::JsonSchema;
use serde::Deserialize;

pub const MAX_CODE_EDITS: usize = 128;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeEdit {
    /// Nonempty exact text, unique in the original file. Include context to disambiguate.
    pub find: String,
    /// Replacement text; empty deletes the matched text.
    pub replace: String,
}

/// All matches address the original revision, never the output of another edit.
/// No filesystem effects occur here, including when the last edit is invalid.
pub fn apply_edits(original: &str, edits: &[CodeEdit], max_bytes: usize) -> Result<String, String> {
    if edits.is_empty() || edits.len() > MAX_CODE_EDITS {
        return Err(format!(
            "edits must contain 1..={MAX_CODE_EDITS} replacements"
        ));
    }
    let mut payload_bytes = 0usize;
    let mut next_bytes = original.len();
    let mut ranges = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        payload_bytes = payload_bytes
            .checked_add(edit.find.len())
            .and_then(|size| size.checked_add(edit.replace.len()))
            .filter(|size| *size <= max_bytes)
            .ok_or_else(|| format!("edit payload exceeds {max_bytes} bytes"))?;
        if edit.find.is_empty() {
            return Err(format!("edits[{index}].find must not be empty"));
        }
        let start = original.find(&edit.find).ok_or_else(|| {
            format!("edits[{index}].find is not present in the original revision")
        })?;
        // Search one UTF-8 character after the first start to catch overlapping
        // occurrences too (e.g. `aa` in `aaa`). str::match_indices skips these.
        let next = start
            + original[start..]
                .chars()
                .next()
                .expect("nonempty match")
                .len_utf8();
        if original[next..].contains(&edit.find) {
            return Err(format!(
                "edits[{index}].find is ambiguous; include unique surrounding context"
            ));
        }
        next_bytes = next_bytes
            .checked_sub(edit.find.len())
            .and_then(|size| size.checked_add(edit.replace.len()))
            .ok_or_else(|| "edit result size overflow".to_string())?;
        ranges.push((start, start + edit.find.len(), index));
    }
    ranges.sort_unstable_by_key(|range| range.0);
    for pair in ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(format!(
                "edits[{}] and edits[{}] overlap in the original revision",
                pair[0].2, pair[1].2
            ));
        }
    }
    if next_bytes > max_bytes {
        return Err(format!("edited file exceeds {max_bytes} bytes"));
    }
    let mut output = String::with_capacity(next_bytes);
    let mut cursor = 0;
    for (start, end, index) in ranges {
        output.push_str(&original[cursor..start]);
        output.push_str(&edits[index].replace);
        cursor = end;
    }
    output.push_str(&original[cursor..]);
    Ok(output)
}

pub fn validate_edit_mode(
    edits: Option<&[CodeEdit]>,
    content: Option<&str>,
    find: Option<&str>,
    replace: Option<&str>,
) -> Result<(), String> {
    if edits.is_some() && (content.is_some() || find.is_some() || replace.is_some()) {
        return Err("edits cannot be combined with content, find, or replace".into());
    }
    if let Some(edits) = edits
        && (edits.is_empty() || edits.len() > MAX_CODE_EDITS)
    {
        return Err(format!(
            "edits must contain 1..={MAX_CODE_EDITS} replacements"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(find: &str, replace: &str) -> CodeEdit {
        CodeEdit {
            find: find.into(),
            replace: replace.into(),
        }
    }

    #[test]
    fn unordered_edits_address_original_utf8_and_preserve_crlf() {
        let original = "α one\r\nβ two\r\n";
        assert_eq!(
            apply_edits(original, &[edit("two", "one"), edit("α one", "γ")], 100).unwrap(),
            "γ\r\nβ one\r\n"
        );
        assert_eq!(
            apply_edits("abc", &[edit("a", ""), edit("b", "B")], 100).unwrap(),
            "Bc"
        );
    }

    #[test]
    fn rejects_missing_empty_ambiguous_and_overlapping_matches() {
        for (original, edits) in [
            ("abc", vec![edit("a", "A"), edit("missing", "")]),
            ("abc", vec![edit("", "x")]),
            ("aaa", vec![edit("aa", "x")]),
            ("ααα", vec![edit("αα", "x")]),
            ("abcd", vec![edit("abc", "x"), edit("cd", "y")]),
            ("abc", vec![edit("a", "x"), edit("a", "y")]),
        ] {
            assert!(apply_edits(original, &edits, 100).is_err());
        }
    }

    #[test]
    fn bounds_and_exclusive_mode() {
        assert!(apply_edits("a", &[], 100).is_err());
        assert!(apply_edits("a", &[edit("a", "12345")], 5).is_err());
        assert!(apply_edits("abcdef", &[edit("a", "ABC")], 7).is_err());
        assert!(validate_edit_mode(Some(&[edit("a", "b")]), Some("x"), None, None).is_err());
        assert!(validate_edit_mode(None, None, Some("a"), Some("b")).is_ok());
    }
}
