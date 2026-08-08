//! Binary split tree — port of `shellSplitTree.ts`.

use serde::{Deserialize, Serialize};

use super::session::{FocusDir, SplitDirection, SplitEdge};

pub const RATIO_MIN: f64 = 0.2;
pub const RATIO_MAX: f64 = 0.8;
pub const RATIO_DEFAULT: f64 = 0.5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SplitNode {
    Group {
        id: String,
    },
    Branch {
        id: String,
        direction: SplitBranchDirection,
        /// Share for child `a` in 0..1 (clamped on write).
        ratio: f64,
        a: Box<SplitNode>,
        b: Box<SplitNode>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitBranchDirection {
    Row,
    Column,
}

pub fn clamp_ratio(ratio: f64) -> f64 {
    if !ratio.is_finite() {
        return RATIO_DEFAULT;
    }
    ratio.clamp(RATIO_MIN, RATIO_MAX)
}

pub fn new_split_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}",
        chrono_like_now_base36(),
        &uuid::Uuid::new_v4().simple().to_string()[..5]
    )
}

fn chrono_like_now_base36() -> String {
    // Match Home's Date.now().toString(36) shape loosely (unique enough with uuid suffix).
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    to_base36(ms)
}

fn to_base36(mut n: u128) -> String {
    if n == 0 {
        return "0".to_string();
    }
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_else(|_| "0".to_string())
}

pub fn count_leaves(node: &SplitNode) -> usize {
    match node {
        SplitNode::Group { .. } => 1,
        SplitNode::Branch { a, b, .. } => count_leaves(a) + count_leaves(b),
    }
}

pub fn collect_group_ids(node: &SplitNode) -> Vec<String> {
    match node {
        SplitNode::Group { id } => vec![id.clone()],
        SplitNode::Branch { a, b, .. } => {
            let mut ids = collect_group_ids(a);
            ids.extend(collect_group_ids(b));
            ids
        }
    }
}

pub fn find_group_leaf(node: &SplitNode, group_id: &str) -> bool {
    match node {
        SplitNode::Group { id } => id == group_id,
        SplitNode::Branch { a, b, .. } => {
            find_group_leaf(a, group_id) || find_group_leaf(b, group_id)
        }
    }
}

/// Split the leaf so the new group lands on `edge` of the host.
pub fn split_leaf_at_edge(
    root: &SplitNode,
    group_id: &str,
    edge: SplitEdge,
    new_group_id: String,
) -> Option<(SplitNode, String)> {
    let branch_direction = match edge {
        SplitEdge::Left | SplitEdge::Right => SplitBranchDirection::Column,
        SplitEdge::Top | SplitEdge::Bottom => SplitBranchDirection::Row,
    };
    let new_first = matches!(edge, SplitEdge::Left | SplitEdge::Top);
    let new_leaf = SplitNode::Group {
        id: new_group_id.clone(),
    };

    fn walk(
        node: &SplitNode,
        group_id: &str,
        branch_direction: SplitBranchDirection,
        new_first: bool,
        new_leaf: &SplitNode,
    ) -> Option<SplitNode> {
        match node {
            SplitNode::Group { id } => {
                if id != group_id {
                    return None;
                }
                let (a, b) = if new_first {
                    (new_leaf.clone(), node.clone())
                } else {
                    (node.clone(), new_leaf.clone())
                };
                Some(SplitNode::Branch {
                    id: new_split_id("branch"),
                    direction: branch_direction,
                    ratio: RATIO_DEFAULT,
                    a: Box::new(a),
                    b: Box::new(b),
                })
            }
            SplitNode::Branch {
                id,
                direction,
                ratio,
                a,
                b,
            } => {
                if let Some(next_a) = walk(a, group_id, branch_direction, new_first, new_leaf) {
                    return Some(SplitNode::Branch {
                        id: id.clone(),
                        direction: *direction,
                        ratio: *ratio,
                        a: Box::new(next_a),
                        b: b.clone(),
                    });
                }
                if let Some(next_b) = walk(b, group_id, branch_direction, new_first, new_leaf) {
                    return Some(SplitNode::Branch {
                        id: id.clone(),
                        direction: *direction,
                        ratio: *ratio,
                        a: a.clone(),
                        b: Box::new(next_b),
                    });
                }
                None
            }
        }
    }

    let next = walk(root, group_id, branch_direction, new_first, &new_leaf)?;
    Some((next, new_group_id))
}

/// Split the leaf `group_id` into a branch; returns new root + new group id.
pub fn split_leaf(
    root: &SplitNode,
    group_id: &str,
    direction: SplitDirection,
    new_group_id: String,
) -> Option<(SplitNode, String)> {
    let edge = match direction {
        SplitDirection::Right => SplitEdge::Right,
        SplitDirection::Down => SplitEdge::Bottom,
    };
    split_leaf_at_edge(root, group_id, edge, new_group_id)
}

/// Leaf group that should receive tabs when `group_id` is closed/merged.
pub fn merge_target_for_leaf(root: &SplitNode, group_id: &str) -> Option<String> {
    fn walk(node: &SplitNode, group_id: &str) -> Option<String> {
        match node {
            SplitNode::Group { .. } => None,
            SplitNode::Branch { a, b, .. } => {
                if let SplitNode::Group { id } = a.as_ref()
                    && id == group_id
                {
                    return collect_group_ids(b).into_iter().next();
                }
                if let SplitNode::Group { id } = b.as_ref()
                    && id == group_id
                {
                    return collect_group_ids(a).into_iter().next_back();
                }
                walk(a, group_id).or_else(|| walk(b, group_id))
            }
        }
    }
    walk(root, group_id)
}

/// Remove leaf `group_id` and promote its sibling.
/// Returns `(root, removed)` — `removed` is false if the leaf is the only pane.
pub fn remove_leaf(root: &SplitNode, group_id: &str) -> (SplitNode, bool) {
    match root {
        SplitNode::Group { .. } => (root.clone(), false),
        SplitNode::Branch { a, b, id, direction, ratio } => {
            if let SplitNode::Group { id: aid } = a.as_ref()
                && aid == group_id
            {
                return (b.as_ref().clone(), true);
            }
            if let SplitNode::Group { id: bid } = b.as_ref()
                && bid == group_id
            {
                return (a.as_ref().clone(), true);
            }
            let (left_root, left_removed) = remove_leaf(a, group_id);
            if left_removed {
                return (
                    SplitNode::Branch {
                        id: id.clone(),
                        direction: *direction,
                        ratio: *ratio,
                        a: Box::new(left_root),
                        b: b.clone(),
                    },
                    true,
                );
            }
            let (right_root, right_removed) = remove_leaf(b, group_id);
            if right_removed {
                return (
                    SplitNode::Branch {
                        id: id.clone(),
                        direction: *direction,
                        ratio: *ratio,
                        a: a.clone(),
                        b: Box::new(right_root),
                    },
                    true,
                );
            }
            (root.clone(), false)
        }
    }
}

pub fn set_branch_ratio(root: &SplitNode, branch_id: &str, ratio: f64) -> SplitNode {
    let next_ratio = clamp_ratio(ratio);
    match root {
        SplitNode::Group { .. } => root.clone(),
        SplitNode::Branch {
            id,
            direction,
            ratio: _,
            a,
            b,
        } if id == branch_id => SplitNode::Branch {
            id: id.clone(),
            direction: *direction,
            ratio: next_ratio,
            a: a.clone(),
            b: b.clone(),
        },
        SplitNode::Branch {
            id,
            direction,
            ratio,
            a,
            b,
        } => SplitNode::Branch {
            id: id.clone(),
            direction: *direction,
            ratio: *ratio,
            a: Box::new(set_branch_ratio(a, branch_id, next_ratio)),
            b: Box::new(set_branch_ratio(b, branch_id, next_ratio)),
        },
    }
}

/// Flat leaf order: depth-first, a then b.
pub fn leaf_order(node: &SplitNode) -> Vec<String> {
    collect_group_ids(node)
}

pub fn neighbor_in_direction(
    root: &SplitNode,
    group_id: &str,
    dir: FocusDir,
) -> Option<String> {
    let order = leaf_order(root);
    let idx = order.iter().position(|id| id == group_id)?;
    let step: isize = match dir {
        FocusDir::Left | FocusDir::Up => -1,
        FocusDir::Right | FocusDir::Down => 1,
    };
    let next = idx as isize + step;
    if next < 0 || next as usize >= order.len() {
        None
    } else {
        Some(order[next as usize].clone())
    }
}

pub fn migrate_v1_to_split_root(group_id: impl Into<String>) -> SplitNode {
    SplitNode::Group {
        id: group_id.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(id: &str) -> SplitNode {
        SplitNode::Group {
            id: id.to_string(),
        }
    }

    #[test]
    fn clamp_ratio_bounds() {
        assert_eq!(clamp_ratio(0.0), RATIO_MIN);
        assert_eq!(clamp_ratio(1.0), RATIO_MAX);
        assert_eq!(clamp_ratio(f64::NAN), RATIO_DEFAULT);
        assert_eq!(clamp_ratio(0.5), 0.5);
    }

    #[test]
    fn split_right_then_remove_restores_single() {
        let root = g("main");
        let (split, new_id) =
            split_leaf(&root, "main", SplitDirection::Right, "pane-2".into()).unwrap();
        assert_eq!(new_id, "pane-2");
        assert_eq!(count_leaves(&split), 2);
        assert_eq!(leaf_order(&split), vec!["main".to_string(), "pane-2".to_string()]);

        let (merged, removed) = remove_leaf(&split, "pane-2");
        assert!(removed);
        assert_eq!(count_leaves(&merged), 1);
        assert_eq!(leaf_order(&merged), vec!["main".to_string()]);
    }

    #[test]
    fn split_down_places_new_below() {
        let root = g("main");
        let (split, _) =
            split_leaf(&root, "main", SplitDirection::Down, "below".into()).unwrap();
        match &split {
            SplitNode::Branch {
                direction: SplitBranchDirection::Row,
                a,
                b,
                ..
            } => {
                assert_eq!(collect_group_ids(a), vec!["main".to_string()]);
                assert_eq!(collect_group_ids(b), vec!["below".to_string()]);
            }
            _ => panic!("expected row branch"),
        }
    }

    #[test]
    fn split_left_edge_puts_new_first() {
        let root = g("main");
        let (split, _) =
            split_leaf_at_edge(&root, "main", SplitEdge::Left, "left".into()).unwrap();
        match &split {
            SplitNode::Branch {
                direction: SplitBranchDirection::Column,
                a,
                b,
                ..
            } => {
                assert_eq!(collect_group_ids(a), vec!["left".to_string()]);
                assert_eq!(collect_group_ids(b), vec!["main".to_string()]);
            }
            _ => panic!("expected column branch"),
        }
    }

    #[test]
    fn merge_target_is_sash_adjacent() {
        let root = g("main");
        let (split, _) =
            split_leaf(&root, "main", SplitDirection::Right, "right".into()).unwrap();
        assert_eq!(
            merge_target_for_leaf(&split, "main").as_deref(),
            Some("right")
        );
        assert_eq!(
            merge_target_for_leaf(&split, "right").as_deref(),
            Some("main")
        );
    }

    #[test]
    fn cannot_remove_sole_leaf() {
        let root = g("main");
        let (next, removed) = remove_leaf(&root, "main");
        assert!(!removed);
        assert_eq!(next, root);
    }

    #[test]
    fn neighbor_walks_leaf_order() {
        let root = g("a");
        let (root, _) = split_leaf(&root, "a", SplitDirection::Right, "b".into()).unwrap();
        let (root, _) = split_leaf(&root, "b", SplitDirection::Right, "c".into()).unwrap();
        assert_eq!(
            neighbor_in_direction(&root, "a", FocusDir::Right).as_deref(),
            Some("b")
        );
        assert_eq!(
            neighbor_in_direction(&root, "b", FocusDir::Left).as_deref(),
            Some("a")
        );
        assert_eq!(neighbor_in_direction(&root, "a", FocusDir::Left), None);
    }

    #[test]
    fn set_branch_ratio_clamps() {
        let root = g("main");
        let (split, _) =
            split_leaf(&root, "main", SplitDirection::Right, "right".into()).unwrap();
        let branch_id = match &split {
            SplitNode::Branch { id, .. } => id.clone(),
            _ => panic!("branch"),
        };
        let next = set_branch_ratio(&split, &branch_id, 0.05);
        match next {
            SplitNode::Branch { ratio, .. } => assert_eq!(ratio, RATIO_MIN),
            _ => panic!("branch"),
        }
    }
}
