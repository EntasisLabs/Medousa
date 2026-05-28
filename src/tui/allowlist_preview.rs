use std::collections::HashSet;

use crate::{
    settings_guard::{invalid_module_ids, parse_allowed_modules},
    tools::extract_module_ops_from_source,
};

#[derive(Debug, Clone)]
pub struct AllowlistPreviewAnalysis {
    pub referenced_ops: Vec<String>,
    pub blocked_ops: Vec<String>,
    pub invalid_allowlist: Vec<String>,
}

pub fn analyze_allowlist_preview(
    source: &str,
    allowed_modules_csv: &str,
) -> AllowlistPreviewAnalysis {
    let referenced_ops = extract_module_ops_from_source(source);
    let allowed_modules = parse_allowed_modules(allowed_modules_csv);
    let invalid_allowlist = invalid_module_ids(&allowed_modules);

    if !invalid_allowlist.is_empty() {
        return AllowlistPreviewAnalysis {
            referenced_ops,
            blocked_ops: Vec::new(),
            invalid_allowlist,
        };
    }

    if allowed_modules.is_empty() {
        return AllowlistPreviewAnalysis {
            referenced_ops,
            blocked_ops: Vec::new(),
            invalid_allowlist,
        };
    }

    let allowed_set = allowed_modules.into_iter().collect::<HashSet<_>>();
    let blocked_ops = referenced_ops
        .iter()
        .filter(|op| !allowed_set.contains(op.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    AllowlistPreviewAnalysis {
        referenced_ops,
        blocked_ops,
        invalid_allowlist,
    }
}

#[cfg(test)]
mod tests {
    use super::analyze_allowlist_preview;

    #[test]
    fn allows_all_when_allowlist_is_empty() {
        let analysis =
            analyze_allowlist_preview("query Run { websearch.search(query: \"x\") { ok } }", "");

        assert_eq!(analysis.referenced_ops, vec!["websearch.search"]);
        assert!(analysis.blocked_ops.is_empty());
        assert!(analysis.invalid_allowlist.is_empty());
    }

    #[test]
    fn reports_blocked_ops_for_non_matching_allowlist() {
        let analysis = analyze_allowlist_preview(
            "query Run { websearch.search(query: \"x\") { ok } }",
            "http.fetch",
        );

        assert_eq!(analysis.referenced_ops, vec!["websearch.search"]);
        assert_eq!(analysis.blocked_ops, vec!["websearch.search"]);
        assert!(analysis.invalid_allowlist.is_empty());
    }
}
