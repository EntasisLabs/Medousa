//! Daemon-owned navigation layout. This module has no UI or local preference store.
//!
//! The host queues one command at a time per authority/profile, captures the client
//! from that binding, and applies only updates matching its current generation.
//! Every mutation reads the current spec first. The daemon PUT contract has no CAS;
//! another client editing between GET and PUT can still race us. Never retry a
//! failed PUT automatically: its remote outcome may be unknown.
use medousa_sdk::MedousaClient;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Command {
    /// Opaque workshop authority identity, echoed unchanged (not an HTTP URL).
    pub authority: String,
    /// Host request/binding generation; also echoed unchanged.
    pub generation: u64,
    pub profile_id: Option<String>,
    pub operation: Operation,
}

#[derive(Clone, Debug)]
pub enum Operation {
    Read,
    SetBotPinned {
        bot_id: String,
        pinned: bool,
    },
    SetBotAccent {
        bot_id: String,
        accent: Option<String>,
    },
    SetVisibility {
        surface_id: String,
        visible: bool,
    },
    /// Insert before a visible movable destination; None means before safety doors.
    Reorder {
        surface_id: String,
        before_surface_id: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct Update {
    pub command: Command,
    pub result: Result<Snapshot, String>,
}
impl Update {
    pub fn matches(&self, authority: &str, generation: u64, profile_id: Option<&str>) -> bool {
        self.command.authority == authority
            && self.command.generation == generation
            && profile(self.command.profile_id.as_deref()) == profile(profile_id)
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    /// Kept opaque so unrelated/custom component configuration is not dropped.
    pub spec: Value,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub visible: bool,
    pub toggleable: bool,
}

fn profile(value: Option<&str>) -> &str {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(medousa_types::environment_default::DEFAULT_PROFILE_ID)
}
const NATIVE_ONLY: [(&str, &str, &str); 3] = [
    ("create", "Create", "pen-tool"),
    ("reviews", "Reviews", "reviews"),
    ("terminal", "Terminal", "terminal"),
];
fn native_only(id: &str) -> bool {
    NATIVE_ONLY.iter().any(|(key, _, _)| *key == id)
}
fn has_surface(spec: &Value, id: &str) -> bool {
    spec["surfaces"]
        .as_array()
        .is_some_and(|s| s.iter().any(|s| s["id"] == id))
}
fn safety(id: &str) -> bool {
    matches!(id, "settings" | "runtime")
}
pub fn toggleable(id: &str) -> bool {
    id != "home" && !safety(id)
}
fn active_index(spec: &Value) -> Option<usize> {
    let presets = spec["layoutPresets"].as_array()?;
    presets
        .iter()
        .position(|p| p["active"].as_bool() == Some(true))
        .or_else(|| {
            presets
                .iter()
                .position(|p| p["id"].as_str().is_some() && p["id"] == spec["activePresetId"])
        })
}
fn ids(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
impl Snapshot {
    pub fn bot_pins(&self) -> Vec<String> {
        ids(&self.spec["nav"]["botPins"])
    }
    pub fn bot_accent(&self, bot_id: &str) -> Option<String> {
        self.spec["nav"]["botAccents"][bot_id]
            .as_str()
            .map(str::to_owned)
    }
    fn shared_visible_ids(&self) -> Vec<String> {
        let mut order = if let Some(index) = active_index(&self.spec) {
            ids(&self.spec["layoutPresets"][index]["surfaces"])
        } else {
            self.spec["surfaces"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| s["id"].as_str())
                .map(str::to_owned)
                .collect()
        };
        let exists = |id: &str| {
            self.spec["surfaces"]
                .as_array()
                .is_some_and(|s| s.iter().any(|s| s["id"] == id))
        };
        order.retain(|id| exists(id));
        // Same safety floor as environment.navSurfaces().
        for id in ["settings", "runtime"] {
            if exists(id) && !order.iter().any(|s| s == id) {
                order.push(id.into());
            }
        }
        order
    }
    pub fn visible_ids(&self) -> Vec<String> {
        let mut order = self.shared_visible_ids();
        let hidden = ids(&self.spec["nav"]["nativeHidden"]);
        for (id, _, _) in NATIVE_ONLY {
            if !has_surface(&self.spec, id) && !hidden.iter().any(|s| s == id) {
                let at = order.iter().position(|s| safety(s)).unwrap_or(order.len());
                order.insert(at, id.into());
            }
        }
        let preferred = ids(&self.spec["nav"]["nativeOrder"]);
        if !preferred.is_empty() {
            // Unknown/deleted override IDs never become invented shared surfaces.
            let mut merged: Vec<String> = preferred
                .into_iter()
                .filter(|id| order.contains(id) && id != "home" && !safety(id))
                .collect();
            for id in &order {
                if id != "home" && !safety(id) && !merged.contains(id) {
                    merged.push(id.clone());
                }
            }
            if order.iter().any(|id| id == "home") {
                merged.insert(0, "home".into());
            }
            for id in &order {
                if safety(id) {
                    merged.push(id.clone());
                }
            }
            order = merged;
        }
        order
    }
    /// Visible entries in preset order, followed by hidden entries in spec order.
    pub fn destinations(&self) -> Vec<Destination> {
        let visible = self.visible_ids();
        let mut all = self.spec["surfaces"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for (id, label, icon) in NATIVE_ONLY {
            if !has_surface(&self.spec, id) {
                all.push(json!({"id":id,"label":label,"icon":icon}));
            }
        }
        let mut order = visible.clone();
        for surface in &all {
            if let Some(id) = surface["id"].as_str() {
                if !order.iter().any(|s| s == id) {
                    order.push(id.into());
                }
            }
        }
        order
            .into_iter()
            .filter_map(|id| {
                let s = all.iter().find(|s| s["id"] == id)?;
                Some(Destination {
                    label: s["label"].as_str().unwrap_or(&id).into(),
                    icon: s["icon"].as_str().map(str::to_owned),
                    visible: visible.contains(&id),
                    toggleable: toggleable(&id),
                    id,
                })
            })
            .collect()
    }
}

/// Modify the active preset or explicit navigation metadata. Return false for a no-op.
pub fn mutate(spec: &mut Value, operation: &Operation) -> Result<bool, String> {
    match operation {
        Operation::SetVisibility {
            surface_id,
            visible,
        } if native_only(surface_id) && !has_surface(spec, surface_id) => {
            let mut hidden = ids(&spec["nav"]["nativeHidden"]);
            let before = hidden.clone();
            hidden.retain(|id| id != surface_id);
            if !visible {
                hidden.push(surface_id.clone());
            }
            if before == hidden {
                return Ok(false);
            }
            ensure_nav(spec)?;
            spec["nav"]["nativeHidden"] = json!(hidden);
            return Ok(true);
        }
        Operation::Reorder {
            surface_id,
            before_surface_id,
        } if (native_only(surface_id) && !has_surface(spec, surface_id))
            || before_surface_id
                .as_deref()
                .is_some_and(|id| native_only(id) && !has_surface(spec, id))
            || !ids(&spec["nav"]["nativeOrder"]).is_empty() =>
        {
            return mutate_native_order(spec, surface_id, before_surface_id.as_deref());
        }
        _ => {}
    }
    mutate_shared(spec, operation)
}
fn mutate_shared(spec: &mut Value, operation: &Operation) -> Result<bool, String> {
    if matches!(
        operation,
        Operation::SetBotPinned { .. } | Operation::SetBotAccent { .. }
    ) {
        return mutate_bot(spec, operation);
    }
    let (id, before) = match operation {
        Operation::Read => return Ok(false),
        Operation::SetBotPinned { .. } | Operation::SetBotAccent { .. } => unreachable!(),
        Operation::SetVisibility { surface_id, .. } => (surface_id.as_str(), None),
        Operation::Reorder {
            surface_id,
            before_surface_id,
        } => (surface_id.as_str(), before_surface_id.as_deref()),
    };
    if !toggleable(id) {
        return Err(format!("Surface '{id}' cannot be hidden or reordered."));
    }
    if !spec["surfaces"]
        .as_array()
        .is_some_and(|s| s.iter().any(|s| s["id"] == id))
    {
        return Err(format!("Unknown surface '{id}'."));
    }
    let index = active_index(spec).ok_or("No active layout preset.")?;
    let value = &spec["layoutPresets"][index]["surfaces"];
    if !value
        .as_array()
        .is_some_and(|a| a.iter().all(Value::is_string))
    {
        return Err("Invalid active layout preset surfaces.".into());
    }
    let original = ids(value);
    let mut next = original.clone();
    match operation {
        Operation::Read | Operation::SetBotPinned { .. } | Operation::SetBotAccent { .. } => {
            unreachable!()
        }
        Operation::SetVisibility { visible: false, .. } => next.retain(|s| s != id),
        Operation::SetVisibility { visible: true, .. } => {
            if !next.iter().any(|s| s == id) {
                let at = next.iter().position(|s| safety(s)).unwrap_or(next.len());
                next.insert(at, id.into());
            }
        }
        Operation::Reorder { .. } => {
            if before == Some(id) {
                return Ok(false);
            }
            if before.is_some_and(|s| !toggleable(s)) {
                return Err("Home and safety destinations cannot be reorder targets.".into());
            }
            next.retain(|s| s != id);
            let at = before
                .and_then(|target| next.iter().position(|s| s == target))
                .or_else(|| next.iter().position(|s| safety(s)))
                .unwrap_or(next.len());
            next.insert(at, id.into());
        }
    }
    if next == original {
        return Ok(false);
    }
    spec["layoutPresets"][index]["surfaces"] = json!(next);
    Ok(true)
}
fn ensure_nav(spec: &mut Value) -> Result<(), String> {
    if spec["nav"].is_null() {
        spec["nav"] = json!({"botPins":[],"botAccents":{}});
    }
    if !spec["nav"].is_object() {
        return Err("Invalid navigation metadata.".into());
    }
    Ok(())
}
fn mutate_native_order(spec: &mut Value, id: &str, before: Option<&str>) -> Result<bool, String> {
    if !toggleable(id) || (!has_surface(spec, id) && !native_only(id)) {
        return Err(format!("Cannot reorder destination '{id}'."));
    }
    if before == Some(id) {
        return Ok(false);
    }
    if before.is_some_and(|target| {
        !toggleable(target) || (!has_surface(spec, target) && !native_only(target))
    }) {
        return Err("Invalid destination reorder target.".into());
    }
    let original = spec.clone();
    let mut next_spec = spec.clone();
    // Reordering an available destination also shows it, matching the shared editor.
    mutate(
        &mut next_spec,
        &Operation::SetVisibility {
            surface_id: id.into(),
            visible: true,
        },
    )?;
    let snapshot = Snapshot {
        spec: next_spec.clone(),
        revision: 0,
    };
    let mut order = snapshot.visible_ids();
    order.retain(|key| key != id);
    let at = before
        .and_then(|target| order.iter().position(|key| key == target))
        .or_else(|| order.iter().position(|key| safety(key)))
        .unwrap_or(order.len());
    order.insert(at, id.into());
    if let Some(index) = active_index(&next_spec) {
        // Shared destinations keep the same relative order in Tauri too.
        let shared: Vec<_> = order
            .iter()
            .filter(|key| has_surface(&next_spec, key))
            .cloned()
            .collect();
        next_spec["layoutPresets"][index]["surfaces"] = json!(shared);
    }
    ensure_nav(&mut next_spec)?;
    next_spec["nav"]["nativeOrder"] = json!(order);
    *spec = next_spec;
    Ok(*spec != original)
}

fn mutate_bot(spec: &mut Value, operation: &Operation) -> Result<bool, String> {
    let bot_id = match operation {
        Operation::SetBotPinned { bot_id, .. } | Operation::SetBotAccent { bot_id, .. } => bot_id,
        _ => unreachable!(),
    };
    if bot_id.is_empty()
        || bot_id.len() > 256
        || bot_id.trim() != bot_id
        || bot_id.chars().any(char::is_control)
    {
        return Err("Invalid Bot ID.".into());
    }
    let mut nav = spec
        .get("nav")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or_else(|| json!({"botPins":[],"botAccents":{}}));
    if !nav.is_object() {
        return Err("Invalid navigation metadata.".into());
    }
    match operation {
        Operation::SetBotPinned { pinned, .. } => {
            let mut pins = ids(&nav["botPins"]);
            if *pinned && !pins.contains(bot_id) {
                pins.push(bot_id.clone());
            }
            if !pinned {
                pins.retain(|id| id != bot_id);
            }
            if pins.len() > 256 {
                return Err("Too many pinned Bots.".into());
            }
            nav["botPins"] = json!(pins);
        }
        Operation::SetBotAccent { accent, .. } => {
            if let Some(color) = accent {
                if color.len() != 7
                    || !color.starts_with('#')
                    || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
                {
                    return Err("Bot accent must be #RRGGBB.".into());
                }
            }
            if nav["botAccents"].is_null() {
                nav["botAccents"] = json!({});
            }
            let accents = nav["botAccents"]
                .as_object_mut()
                .ok_or("Invalid Bot accent map.")?;
            if let Some(color) = accent {
                accents.insert(bot_id.clone(), json!(color));
            } else {
                accents.remove(bot_id);
            }
            if accents.len() > 256 {
                return Err("Too many Bot accents.".into());
            }
        }
        _ => unreachable!(),
    }
    if spec.get("nav") == Some(&nav) {
        return Ok(false);
    }
    spec["nav"] = nav;
    Ok(true)
}

fn decode(value: Value, requested_profile: Option<&str>) -> Result<Snapshot, String> {
    let revision = value["revision"]
        .as_u64()
        .ok_or("Missing environment revision.")?;
    let spec = value
        .get("spec")
        .filter(|s| s.is_object())
        .ok_or("Missing environment spec.")?
        .clone();
    if spec["profileId"].as_str() != Some(profile(requested_profile)) {
        return Err("Environment response belongs to another profile.".into());
    }
    if !spec["surfaces"].is_array() {
        return Err("Invalid environment surfaces.".into());
    }
    Ok(Snapshot { spec, revision })
}

/// Use the already-authenticated authority-bound client. No ambient/local fallback.
pub async fn execute(client: &Arc<MedousaClient>, command: Command) -> Update {
    let result = execute_inner(client, &command).await;
    Update { command, result }
}
async fn execute_inner(client: &MedousaClient, command: &Command) -> Result<Snapshot, String> {
    if command.authority.trim().is_empty() {
        return Err("Missing workshop authority.".into());
    }
    let path = format!(
        "/v1/environment/spec?profile_id={}",
        urlencoding::encode(profile(command.profile_id.as_deref()))
    );
    let response: Value = client.http().get(&path).await.map_err(|e| e.to_string())?;
    let mut snapshot = decode(response, command.profile_id.as_deref())?;
    if !mutate(&mut snapshot.spec, &command.operation)? {
        return Ok(snapshot);
    }
    let response: Value = client
        .http()
        .put("/v1/environment/spec", &json!({"spec": snapshot.spec}))
        .await
        .map_err(|e| {
            format!("Navigation save was not acknowledged; refresh before retrying: {e}")
        })?;
    let saved = decode(response, command.profile_id.as_deref())?;
    // An older daemon can accept a PUT while stripping unrecognized nav fields.
    // Acknowledgement is not proof that the requested preference survived.
    let mut verification = saved.spec.clone();
    if mutate(&mut verification, &command.operation)? {
        return Err("The daemon did not retain this navigation change. Update/restart the workshop daemon, then refresh navigation before retrying.".into());
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec() -> Value {
        json!({"profileId":"personal","future":{"x":1},"surfaces":[
        {"id":"home"},{"id":"chat","label":"Chat","icon":"message-circle"},
        {"id":"notes"},{"id":"settings"},{"id":"runtime"}],
        "components":[{"config":{"unknown":"keep"}}],
        "activePresetId":"fallback","layoutPresets":[
        {"id":"fallback","surfaces":["notes"],"theme":{"custom":true}},
        {"id":"active","active":true,"surfaces":["home","chat","settings","runtime"],"unknown":2}]})
    }
    fn show(id: &str, visible: bool) -> Operation {
        Operation::SetVisibility {
            surface_id: id.into(),
            visible,
        }
    }
    #[test]
    fn visibility_uses_active_flag_and_preserves_every_other_field() {
        let mut v = spec();
        let original = v.clone();
        assert!(mutate(&mut v, &show("notes", true)).unwrap());
        assert_eq!(
            v["layoutPresets"][1]["surfaces"],
            json!(["home", "chat", "notes", "settings", "runtime"])
        );
        v["layoutPresets"][1]["surfaces"] = original["layoutPresets"][1]["surfaces"].clone();
        assert_eq!(v, original);
    }
    #[test]
    fn visibility_noop_hide_and_safety_rejection() {
        let mut v = spec();
        assert!(!mutate(&mut v, &show("chat", true)).unwrap());
        assert!(mutate(&mut v, &show("chat", false)).unwrap());
        assert!(!mutate(&mut v, &show("chat", false)).unwrap());
        for id in ["home", "settings", "runtime", "absent"] {
            assert!(mutate(&mut v, &show(id, false)).is_err());
        }
    }
    #[test]
    fn reorder_inserts_hidden_and_keeps_safety_at_end() {
        let mut v = spec();
        mutate(
            &mut v,
            &Operation::Reorder {
                surface_id: "notes".into(),
                before_surface_id: Some("chat".into()),
            },
        )
        .unwrap();
        assert_eq!(
            v["layoutPresets"][1]["surfaces"],
            json!(["home", "notes", "chat", "settings", "runtime"])
        );
        mutate(
            &mut v,
            &Operation::Reorder {
                surface_id: "notes".into(),
                before_surface_id: None,
            },
        )
        .unwrap();
        assert_eq!(
            v["layoutPresets"][1]["surfaces"],
            json!(["home", "chat", "notes", "settings", "runtime"])
        );
    }
    #[test]
    fn fallback_projection_includes_safety_and_hidden_entries() {
        let mut v = spec();
        v["layoutPresets"][1]["active"] = json!(false);
        let snap = Snapshot {
            spec: v,
            revision: 3,
        };
        assert_eq!(
            snap.visible_ids(),
            vec![
                "notes", "create", "reviews", "terminal", "settings", "runtime"
            ]
        );
        assert_eq!(
            snap.destinations()
                .iter()
                .map(|s| (s.id.as_str(), s.visible))
                .collect::<Vec<_>>(),
            vec![
                ("notes", true),
                ("create", true),
                ("reviews", true),
                ("terminal", true),
                ("settings", true),
                ("runtime", true),
                ("home", false),
                ("chat", false)
            ]
        );
    }
    #[test]
    fn no_preset_is_readable_but_not_mutable() {
        let mut v = spec();
        v.as_object_mut().unwrap().remove("layoutPresets");
        assert_eq!(
            Snapshot {
                spec: v.clone(),
                revision: 1
            }
            .visible_ids()
            .len(),
            8
        );
        assert!(mutate(&mut v, &show("chat", false)).is_err());
    }
    #[test]
    fn native_only_visibility_never_invents_shared_surfaces() {
        let mut v = spec();
        let surfaces = v["surfaces"].clone();
        let presets = v["layoutPresets"].clone();
        for id in ["create", "reviews", "terminal"] {
            assert!(
                Snapshot {
                    spec: v.clone(),
                    revision: 0
                }
                .visible_ids()
                .contains(&id.into())
            );
            assert!(mutate(&mut v, &show(id, false)).unwrap());
            let snap = Snapshot {
                spec: v.clone(),
                revision: 0,
            };
            assert!(!snap.visible_ids().contains(&id.into()));
            assert!(
                !snap
                    .destinations()
                    .into_iter()
                    .find(|d| d.id == id)
                    .unwrap()
                    .visible
            );
            assert!(mutate(&mut v, &show(id, true)).unwrap());
        }
        assert_eq!(v["surfaces"], surfaces);
        assert_eq!(v["layoutPresets"], presets);
    }
    #[test]
    fn native_reorder_anchors_shared_and_native_destinations() {
        let mut v = spec();
        mutate(
            &mut v,
            &Operation::Reorder {
                surface_id: "terminal".into(),
                before_surface_id: Some("chat".into()),
            },
        )
        .unwrap();
        assert_eq!(
            Snapshot {
                spec: v.clone(),
                revision: 0
            }
            .visible_ids(),
            vec![
                "home", "terminal", "chat", "create", "reviews", "settings", "runtime"
            ]
        );
        assert_eq!(
            v["layoutPresets"][1]["surfaces"],
            json!(["home", "chat", "settings", "runtime"])
        );
        mutate(
            &mut v,
            &Operation::Reorder {
                surface_id: "notes".into(),
                before_surface_id: Some("terminal".into()),
            },
        )
        .unwrap();
        assert_eq!(
            Snapshot {
                spec: v.clone(),
                revision: 0
            }
            .visible_ids(),
            vec![
                "home", "notes", "terminal", "chat", "create", "reviews", "settings", "runtime"
            ]
        );
        assert_eq!(
            v["layoutPresets"][1]["surfaces"],
            json!(["home", "notes", "chat", "settings", "runtime"])
        );
        assert!(
            mutate(
                &mut v,
                &Operation::Reorder {
                    surface_id: "terminal".into(),
                    before_surface_id: Some("home".into())
                }
            )
            .is_err()
        );
    }
    #[test]
    fn bot_pins_and_accents_preserve_spec_and_other_metadata() {
        let mut v = spec();
        v["nav"] = json!({"botPins":[],"botAccents":{},"future":true});
        let pin = Operation::SetBotPinned {
            bot_id: "bot-one".into(),
            pinned: true,
        };
        assert!(mutate(&mut v, &pin).unwrap());
        assert!(!mutate(&mut v, &pin).unwrap());
        mutate(
            &mut v,
            &Operation::SetBotAccent {
                bot_id: "bot-one".into(),
                accent: Some("#123aBc".into()),
            },
        )
        .unwrap();
        let snap = Snapshot {
            spec: v.clone(),
            revision: 7,
        };
        assert_eq!(snap.bot_pins(), vec!["bot-one"]);
        assert_eq!(snap.bot_accent("bot-one").as_deref(), Some("#123aBc"));
        mutate(
            &mut v,
            &Operation::SetBotPinned {
                bot_id: "bot-one".into(),
                pinned: false,
            },
        )
        .unwrap();
        assert_eq!(v["nav"]["future"], true);
        assert_eq!(v["nav"]["botAccents"]["bot-one"], "#123aBc");
        assert_eq!(v["components"], spec()["components"]);
        let original = v.clone();
        assert!(
            mutate(
                &mut v,
                &Operation::SetBotAccent {
                    bot_id: "bot-one".into(),
                    accent: Some("red; bad".into())
                }
            )
            .is_err()
        );
        assert_eq!(v, original);
    }
    #[test]
    fn profile_and_request_fencing() {
        let update = Update {
            command: Command {
                authority: "a".into(),
                generation: 4,
                profile_id: None,
                operation: Operation::Read,
            },
            result: Err("offline".into()),
        };
        assert!(update.matches("a", 4, Some("personal")));
        assert!(!update.matches("b", 4, None));
        assert!(!update.matches("a", 5, None));
        assert!(!update.matches("a", 4, Some("other")));
        assert!(decode(json!({"spec":spec(),"revision":4}), Some("other")).is_err());
        assert!(decode(json!({"spec":spec()}), None).is_err());
    }
}
