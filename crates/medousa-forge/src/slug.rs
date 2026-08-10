//! Human-readable slugs for forge worktrees and branches.

use std::collections::HashSet;

/// Lowercase alphanumeric slug from a title; non-alnum runs become `-`.
/// Empty input → `"new-project"`. Truncated to 64 characters.
pub fn project_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            separator = false;
        } else if !slug.is_empty() && !separator {
            slug.push('-');
            separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "new-project".to_string()
    } else {
        slug.chars().take(64).collect()
    }
}

/// Pick `base`, or `base-2`…`base-999`, avoiding any string in `taken`.
pub fn allocate_unique_slug<'a>(
    base: &str,
    taken: impl IntoIterator<Item = &'a str>,
) -> String {
    let taken: HashSet<&str> = taken.into_iter().collect();
    if !taken.contains(base) {
        return base.to_string();
    }
    for suffix in 2..=999 {
        let candidate = format!("{base}-{suffix}");
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
    }
    format!("{base}-x")
}

/// Staging branch: `worktree/{slug}` (re-provision generation > 1 → `-g{n}`).
pub fn staging_branch(slug: &str, generation: u32) -> String {
    if generation <= 1 {
        format!("worktree/{slug}")
    } else {
        format!("worktree/{slug}-g{generation}")
    }
}

/// Attempt branch: `worktree/{slug}-a{seq}` (flat — must not nest under staging ref).
pub fn attempt_branch(slug: &str, attempt_seq: u32) -> String {
    format!("worktree/{slug}-a{attempt_seq}")
}

/// Staging worktree leaf directory name.
pub fn staging_worktree_leaf(slug: &str, generation: u32) -> String {
    if generation <= 1 {
        slug.to_string()
    } else {
        format!("{slug}-g{generation}")
    }
}

/// Attempt worktree leaf directory name.
pub fn attempt_worktree_leaf(slug: &str, attempt_seq: u32) -> String {
    format!("{slug}-a{attempt_seq}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_safe() {
        assert_eq!(
            project_slug(" Personal Finance / Dashboard "),
            "personal-finance-dashboard"
        );
        assert_eq!(project_slug("!!!"), "new-project");
        assert_eq!(
            project_slug("Updating terminal chrome"),
            "updating-terminal-chrome"
        );
    }

    #[test]
    fn allocates_collision_suffixes() {
        let taken = ["updating-terminal-chrome", "updating-terminal-chrome-2"];
        assert_eq!(
            allocate_unique_slug("updating-terminal-chrome", taken),
            "updating-terminal-chrome-3"
        );
        assert_eq!(allocate_unique_slug("fresh", taken), "fresh");
    }

    #[test]
    fn branch_and_leaf_formats() {
        assert_eq!(
            staging_branch("updating-terminal-chrome", 1),
            "worktree/updating-terminal-chrome"
        );
        assert_eq!(
            staging_branch("updating-terminal-chrome", 2),
            "worktree/updating-terminal-chrome-g2"
        );
        assert_eq!(
            attempt_branch("updating-terminal-chrome", 1),
            "worktree/updating-terminal-chrome-a1"
        );
        assert_eq!(
            staging_worktree_leaf("updating-terminal-chrome", 1),
            "updating-terminal-chrome"
        );
        assert_eq!(
            attempt_worktree_leaf("updating-terminal-chrome", 3),
            "updating-terminal-chrome-a3"
        );
    }
}
