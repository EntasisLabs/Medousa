//! Stack-based layout composition for custom surface main bodies (Phase 3).

use serde::{Deserialize, Serialize};

use crate::environment::{ComponentDef, SurfaceDef, SurfaceKind, COMPONENT_SLOT_MAIN};

pub const MAX_LAYOUT_DEPTH: usize = 8;
pub const MAX_LAYOUT_NODES: usize = 32;
pub const MAX_GRID_COLUMNS: u8 = 4;
pub const MAX_COMPONENT_FLEX: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum StackSpacing {
    #[default]
    #[serde(alias = "NONE")]
    None,
    #[serde(alias = "SM")]
    Sm,
    #[serde(alias = "MD")]
    Md,
    #[serde(alias = "LG")]
    Lg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum StackAlign {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum StackDistribution {
    #[default]
    Start,
    Center,
    End,
    #[serde(alias = "spaceBetween")]
    SpaceBetween,
    #[serde(alias = "fillEqually")]
    FillEqually,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "type")]
pub enum LayoutNode {
    #[serde(rename = "vstack", alias = "v_stack")]
    VStack {
        #[serde(default)]
        spacing: StackSpacing,
        #[serde(default)]
        align: StackAlign,
        #[serde(default)]
        distribution: StackDistribution,
        children: Vec<LayoutNode>,
    },
    #[serde(rename = "hstack", alias = "h_stack")]
    HStack {
        #[serde(default)]
        spacing: StackSpacing,
        #[serde(default)]
        align: StackAlign,
        #[serde(default)]
        distribution: StackDistribution,
        children: Vec<LayoutNode>,
    },
    #[serde(rename = "grid")]
    Grid {
        columns: u8,
        #[serde(default)]
        spacing: StackSpacing,
        children: Vec<LayoutNode>,
    },
    #[serde(rename = "component")]
    Component {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        flex: Option<u8>,
    },
    #[serde(rename = "slot")]
    Slot {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        flex: Option<u8>,
    },
}

impl LayoutNode {
    pub fn implicit_vstack(component_ids: impl IntoIterator<Item = String>) -> Self {
        let children = component_ids
            .into_iter()
            .map(|id| LayoutNode::Component { id, flex: None })
            .collect();
        LayoutNode::VStack {
            spacing: StackSpacing::Md,
            align: StackAlign::Start,
            distribution: StackDistribution::Start,
            children,
        }
    }
}

/// Resolve the effective layout tree for a custom surface main body.
pub fn resolve_layout_root(surface: &SurfaceDef, components: &[ComponentDef]) -> LayoutNode {
    if let Some(root) = &surface.layout_root {
        return root.clone();
    }
    let main_ids = components
        .iter()
        .filter(|component| {
            component.surface_id == surface.id && component.slot == COMPONENT_SLOT_MAIN
        })
        .map(|component| component.id.clone())
        .collect::<Vec<_>>();
    let distribution = if surface.layout == crate::environment::SurfaceLayout::Dashboard
        && main_ids.len() > 1
    {
        StackDistribution::FillEqually
    } else {
        StackDistribution::Start
    };
    LayoutNode::VStack {
        spacing: StackSpacing::Md,
        align: StackAlign::Start,
        distribution,
        children: main_ids
            .into_iter()
            .map(|id| LayoutNode::Component { id, flex: None })
            .collect(),
    }
}

pub fn validate_layout_tree(
    surface: &SurfaceDef,
    components: &[ComponentDef],
) -> Vec<String> {
    let mut errors = Vec::new();
    if surface.layout_root.is_none() {
        return errors;
    }
    if surface.kind != SurfaceKind::Custom {
        errors.push(format!(
            "surface '{}' layoutRoot is only allowed on custom surfaces",
            surface.id
        ));
        return errors;
    }
    let Some(root) = &surface.layout_root else {
        return errors;
    };
    let mut seen = std::collections::HashSet::new();
    let mut node_count = 0usize;
    validate_layout_node(
        surface,
        components,
        root,
        1,
        &mut seen,
        &mut node_count,
        &mut errors,
    );
    errors
}

fn validate_layout_node(
    surface: &SurfaceDef,
    components: &[ComponentDef],
    node: &LayoutNode,
    depth: usize,
    seen: &mut std::collections::HashSet<String>,
    node_count: &mut usize,
    errors: &mut Vec<String>,
) {
    *node_count += 1;
    if depth > MAX_LAYOUT_DEPTH {
        errors.push(format!(
            "surface '{}' layoutRoot exceeds max depth ({MAX_LAYOUT_DEPTH})",
            surface.id
        ));
    }
    if *node_count > MAX_LAYOUT_NODES {
        errors.push(format!(
            "surface '{}' layoutRoot exceeds max nodes ({MAX_LAYOUT_NODES})",
            surface.id
        ));
        return;
    }

    match node {
        LayoutNode::VStack { children, .. } | LayoutNode::HStack { children, .. } => {
            if children.is_empty() {
                errors.push(format!(
                    "surface '{}' layout stack requires at least one child",
                    surface.id
                ));
            }
            for child in children {
                validate_layout_node(surface, components, child, depth + 1, seen, node_count, errors);
            }
        }
        LayoutNode::Grid { columns, children, .. } => {
            if *columns == 0 || *columns > MAX_GRID_COLUMNS {
                errors.push(format!(
                    "surface '{}' grid columns must be 1..={MAX_GRID_COLUMNS}",
                    surface.id
                ));
            }
            if children.is_empty() {
                errors.push(format!(
                    "surface '{}' layout grid requires at least one child",
                    surface.id
                ));
            }
            for child in children {
                validate_layout_node(surface, components, child, depth + 1, seen, node_count, errors);
            }
        }
        LayoutNode::Component { id, flex } => {
            if id.trim().is_empty() {
                errors.push(format!(
                    "surface '{}' layout component ref requires id",
                    surface.id
                ));
                return;
            }
            if let Some(flex) = flex {
                if *flex > MAX_COMPONENT_FLEX {
                    errors.push(format!(
                        "surface '{}' component '{id}' flex must be 0..={MAX_COMPONENT_FLEX}",
                        surface.id
                    ));
                }
            }
            if !seen.insert(id.clone()) {
                errors.push(format!(
                    "surface '{}' layout references component '{id}' more than once",
                    surface.id
                ));
            }
            let Some(component) = components.iter().find(|entry| entry.id == *id) else {
                errors.push(format!(
                    "surface '{}' layout references unknown component '{id}'",
                    surface.id
                ));
                return;
            };
            if component.surface_id != surface.id {
                errors.push(format!(
                    "surface '{}' layout component '{id}' belongs to surface '{}'",
                    surface.id, component.surface_id
                ));
            }
            if component.slot != COMPONENT_SLOT_MAIN {
                errors.push(format!(
                    "surface '{}' layout component '{id}' must use slot '{COMPONENT_SLOT_MAIN}' (got '{}')",
                    surface.id, component.slot
                ));
            }
        }
        LayoutNode::Slot { id, flex } => {
            if id.trim().is_empty() {
                errors.push(format!(
                    "surface '{}' layout slot requires id",
                    surface.id
                ));
                return;
            }
            if let Some(flex) = flex {
                if *flex > MAX_COMPONENT_FLEX {
                    errors.push(format!(
                        "surface '{}' slot '{id}' flex must be 0..={MAX_COMPONENT_FLEX}",
                        surface.id
                    ));
                }
            }
            if !seen.insert(format!("slot:{id}")) {
                errors.push(format!(
                    "surface '{}' layout references slot '{id}' more than once",
                    surface.id
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{
        ComponentDef, ComponentType, SurfaceDef, SurfaceLayout, UiPresentation,
    };

    fn custom_surface(id: &str) -> SurfaceDef {
        SurfaceDef {
            id: id.to_string(),
            label: id.to_string(),
            icon: "layout-grid".to_string(),
            kind: SurfaceKind::Custom,
            builtin_id: None,
            layout: SurfaceLayout::Dashboard,
            slots: vec![],
            mobile_tab: None,
            layout_root: None,
        }
    }

    fn main_component(id: &str, surface_id: &str) -> ComponentDef {
        ComponentDef {
            id: id.to_string(),
            component_type: ComponentType::Presentation,
            surface_id: surface_id.to_string(),
            slot: COMPONENT_SLOT_MAIN.to_string(),
            label: None,
            config: serde_json::json!({ "artifactId": "art-demo" }),
            presentation: Some(UiPresentation::Panel),
            feeds: vec![],
            updated_at: None,
        }
    }

    #[test]
    fn resolve_layout_root_implicit_vstack() {
        let surface = custom_surface("adhd-guide");
        let components = vec![
            main_component("a", "adhd-guide"),
            main_component("b", "adhd-guide"),
        ];
        let root = resolve_layout_root(&surface, &components);
        match root {
            LayoutNode::VStack { children, .. } => {
                assert_eq!(children.len(), 2);
            }
            _ => panic!("expected implicit vstack"),
        }
    }

    #[test]
    fn validate_rejects_duplicate_component_refs() {
        let mut surface = custom_surface("studio");
        surface.layout_root = Some(LayoutNode::HStack {
            spacing: StackSpacing::Md,
            align: StackAlign::Start,
            distribution: StackDistribution::FillEqually,
            children: vec![
                LayoutNode::Component {
                    id: "a".to_string(),
                    flex: Some(1),
                },
                LayoutNode::Component {
                    id: "a".to_string(),
                    flex: Some(1),
                },
            ],
        });
        let components = vec![main_component("a", "studio")];
        let errors = validate_layout_tree(&surface, &components);
        assert!(errors.iter().any(|e| e.contains("more than once")));
    }

    #[test]
    fn deserializes_layout_aliases_from_models() {
        let json = serde_json::json!({
            "type": "h_stack",
            "spacing": "md",
            "distribution": "fill_equally",
            "children": [
                { "type": "component", "id": "adhd-guide-tetris", "flex": 1 },
                { "type": "component", "id": "adhd-guide-original", "flex": 1 }
            ]
        });
        let node: LayoutNode = serde_json::from_value(json).expect("h_stack alias");
        assert_eq!(node, LayoutNode::HStack {
            spacing: StackSpacing::Md,
            align: StackAlign::Start,
            distribution: StackDistribution::FillEqually,
            children: vec![
                LayoutNode::Component {
                    id: "adhd-guide-tetris".to_string(),
                    flex: Some(1),
                },
                LayoutNode::Component {
                    id: "adhd-guide-original".to_string(),
                    flex: Some(1),
                },
            ],
        });

        let hstack: LayoutNode = serde_json::from_value(serde_json::json!({
            "type": "hstack",
            "children": [{ "type": "component", "id": "a" }]
        }))
        .expect("hstack alias");
        assert!(matches!(hstack, LayoutNode::HStack { .. }));

        let camel: LayoutNode = serde_json::from_value(serde_json::json!({
            "type": "hstack",
            "distribution": "fillEqually",
            "children": [{ "type": "component", "id": "a" }]
        }))
        .expect("fillEqually alias");
        assert!(matches!(
            camel,
            LayoutNode::HStack {
                distribution: StackDistribution::FillEqually,
                ..
            }
        ));
    }
}
