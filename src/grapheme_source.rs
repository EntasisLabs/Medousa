//! Portable Grapheme source inspection shared by every daemon deployment.

/// Return the unique, sorted `module.operation` calls referenced by source.
pub fn extract_module_ops_from_source(source: &str) -> Vec<String> {
    let mut ops = Vec::new();
    let chars = source.chars().collect::<Vec<_>>();
    let mut idx = 0usize;

    while idx < chars.len() {
        if !chars[idx].is_ascii_alphabetic() && chars[idx] != '_' {
            idx += 1;
            continue;
        }

        let start = idx;
        idx += 1;
        while idx < chars.len() && (chars[idx].is_ascii_alphanumeric() || chars[idx] == '_') {
            idx += 1;
        }
        let left = chars[start..idx].iter().collect::<String>();

        if idx >= chars.len() || chars[idx] != '.' {
            continue;
        }
        idx += 1;

        if idx >= chars.len() || (!chars[idx].is_ascii_alphabetic() && chars[idx] != '_') {
            continue;
        }

        let right_start = idx;
        idx += 1;
        while idx < chars.len() && (chars[idx].is_ascii_alphanumeric() || chars[idx] == '_') {
            idx += 1;
        }
        let right = chars[right_start..idx].iter().collect::<String>();

        let mut lookahead = idx;
        while lookahead < chars.len() && chars[lookahead].is_ascii_whitespace() {
            lookahead += 1;
        }

        if lookahead < chars.len() && chars[lookahead] == '(' {
            ops.push(format!("{left}.{right}"));
        }
    }

    ops.sort();
    ops.dedup();
    ops
}

#[cfg(test)]
mod tests {
    use super::extract_module_ops_from_source;

    #[test]
    fn extracts_unique_module_operation_calls() {
        let source = "query Example { web.fetch(url: \"x\") |> core.echo(message: \"hi\") |> web.fetch(url: \"y\") }";
        assert_eq!(
            extract_module_ops_from_source(source),
            vec!["core.echo".to_string(), "web.fetch".to_string()]
        );
    }
}
