//! Settings content follows the production section components. Effectful controls
//! are explanations, never enabled fake buttons or unqualified secret fields.
use crate::admin_model::{Resource, Route, SETTINGS};
use crate::desktop::{Handles, nav_button};
use crate::presentation::label;
use crate::shell::{muted, row};
use rndr_core::*;

pub fn resource_link(world: &mut World, parent: Entity, resource: Resource) {
    let index = Resource::ALL.iter().position(|r| *r == resource).unwrap();
    nav_button(world, parent, &format!("data:{index}"), resource.title());
}
fn band(world: &mut World, parent: Entity, title: &str, text: &str) {
    let heading = label(world, parent, title, true);
    world.get_mut::<TextStyle>(heading).unwrap().size = 16.;
    muted(world, parent, text);
}
pub fn render(world: &mut World, h: &Handles, route: Route, narrow: bool) {
    if let Route::Settings(index) = route {
        if narrow {
            let r = row(world, h.section_nav, Size::Px(34.));
            nav_button(
                world,
                r,
                &format!("settings:{}", (index + 8) % 9),
                "‹ Previous",
            );
            nav_button(
                world,
                r,
                &format!("settings:{}", (index + 1) % 9),
                "Next section ›",
            );
            muted(
                world,
                h.section_nav,
                &format!("Settings {} of 9 · {}", index + 1, SETTINGS[index].2),
            );
        } else {
            for (i, (_, title, hint)) in SETTINGS.iter().enumerate() {
                if i == 0 {
                    label(world, h.section_nav, "Medousa", true);
                }
                if i == 6 {
                    label(world, h.section_nav, "This host", true);
                }
                let entry = nav_button(
                    world,
                    h.section_nav,
                    &format!("settings:{i}"),
                    &format!("{}{}", if i == index { "• " } else { "" }, title),
                );
                world.insert(
                    entry,
                    Semantics {
                        name: Some(format!(
                            "{title}{} · {hint}",
                            if i == index {
                                ", current settings view"
                            } else {
                                ""
                            }
                        )),
                        ..Default::default()
                    },
                );
                muted(world, h.section_nav, hint);
            }
        }
        settings(world, h.body, index);
    } else if route == Route::Data(Resource::Activity) {
        if !narrow {
            label(world, h.section_nav, "Activity", true);
        }
        nav_button(
            world,
            h.section_nav,
            "desk:schedules",
            "Scheduled automations",
        );
        muted(
            world,
            h.body,
            "Workspace cards: latest 100 including terminal. Not the full job ledger. Titles/output withheld; board state is not execution-success proof.",
        );
    } else if let Route::Data(_) = route {
        if narrow {
            let r = row(world, h.section_nav, Size::Px(34.));
            nav_button(world, r, "settings:3", "Runtime Controls");
            nav_button(world, r, "settings:8", "Connection");
        } else {
            label(world, h.section_nav, "Settings / administration", true);
            nav_button(world, h.section_nav, "settings:3", "Runtime Controls");
            nav_button(world, h.section_nav, "settings:8", "Connection");
            for resource in Resource::ALL {
                resource_link(world, h.section_nav, resource);
            }
        }
    } else if let Route::Deferred(name) = route {
        band(
            world,
            h.body,
            name,
            "Not ported in this preview. No live content is being simulated.",
        );
        muted(
            world,
            h.body,
            match name {
                "Peers" => {
                    "Nearby workshops, trust and inbox require the existing pairing and authorization UI. No discovery, trust or external grants are performed here."
                }
                "Code" | "Reviews" => {
                    "Project editing and sealed integration reviews remain in the existing app. This preview cannot approve, apply or integrate work."
                }
                "You" => {
                    "Identity profiles and account ownership remain in the existing app. No identity settings are persisted here."
                }
                _ => {
                    "This destination retains its production name. Its editor/service workflow is deferred rather than replaced with fixture content. Return to Settings for the implemented administrative views."
                }
            },
        );
        nav_button(world, h.body, "desk:settings", "Open settings");
    }
    if let Some(resource) = route.resource() {
        crate::admin_views::dataset(world, h.body, resource);
    }
}
fn settings(world: &mut World, parent: Entity, index: usize) {
    match index {
        0 => {
            band(
                world,
                parent,
                "Window appearance",
                "Local preview only · changes are reversible and reset when this window closes.",
            );
            for (action, name) in [
                ("appearance:light", "Light"),
                ("appearance:dark", "Dark"),
                ("appearance:contrast", "Toggle high contrast"),
            ] {
                nav_button(world, parent, action, name);
            }
            band(
                world,
                parent,
                "Notifications & chrome",
                "Production preferences are not loaded or changed. Notification permissions, sound, layout presets and global shortcuts remain managed by the existing Medousa app.",
            );
            muted(
                world,
                parent,
                "Physical keyboard, IME, accessibility and device checks are pending user verification. Harmless navigation and local appearance remain available.",
            );
        }
        1 => {
            band(
                world,
                parent,
                "Connected services & review accounts",
                "Read-only daemon-reported external account status below. Unknown is not signed out; binary presence is not authenticated access.",
            );
            muted(
                world,
                parent,
                "Provider keys, OAuth, review-account linking and sign-out use the existing Medousa app's authorization and secret-entry flows. No credential entry is offered in this preview.",
            );
            resource_link(world, parent, Resource::Models);
            resource_link(world, parent, Resource::ReviewProviders);
        }
        2 => {
            band(
                world,
                parent,
                "Models & providers",
                "Cached model catalog only · an entry does not prove a working provider connection or permission to infer. Select an item for its provider and model ID.",
            );
            resource_link(world, parent, Resource::Modes);
            resource_link(world, parent, Resource::LocalEngine);
            band(
                world,
                parent,
                "Stance, memory & routing",
                "Charter, inference profiles, stage routing, model refresh, retention and storage governance are not edited here. Use the existing app; no paid inference or provider validation occurs.",
            );
        }
        3 => {
            band(
                world,
                parent,
                "Reach, shell, engine & versions",
                "Read-only health and version descriptor from the configured loopback daemon. A successful read is a snapshot, not continuous liveness or native-input qualification.",
            );
            resource_link(world, parent, Resource::LocalEngine);
            resource_link(world, parent, Resource::Runtimes);
            resource_link(world, parent, Resource::Schedules);
            resource_link(world, parent, Resource::Capacity);
            muted(
                world,
                parent,
                "Shell reach, computer control, worker capacity, SSH, permissions and runtime defaults remain managed by the existing app. This preview cannot grant access or start workers.",
            );
        }
        4 => {
            band(
                world,
                parent,
                "Seats, phone, peers & channels",
                "Shared-mode status is read below. Pairing, phone and channel status are not yet ported. No remote workshop, adapter or seat is implied to be connected.",
            );
            band(
                world,
                parent,
                "Phone & LAN sharing",
                "Use existing Settings → Sharing to pair devices, configure shared mode or invite seats. These operations grant access and are intentionally absent here.",
            );
            band(
                world,
                parent,
                "Channels",
                "Messaging adapter configuration and credentials stay in the existing app. Installation and connection status are not inferred from package names.",
            );
            nav_button(world, parent, "settings:6", "Packages");
            nav_button(world, parent, "desk:peers", "Peers · port status");
        }
        5 => {
            band(
                world,
                parent,
                "External coding agents & conversations",
                "Inspect daemon-reported runtime availability and account status. Details omit executable commands, auth diagnostics and credentials.",
            );
            muted(
                world,
                parent,
                "Sign-in, sign-out, installation, session adoption and connected conversation setup stay in the existing app. Runtime availability does not authorize execution.",
            );
        }
        6 => {
            band(
                world,
                parent,
                "Optional packages",
                "Package inventory requires the production Tauri host package service; the daemon model catalog is not an installed-package list.",
            );
            band(
                world,
                parent,
                "Install & repair",
                "Not available here. The existing app's Settings → Packages owns optional binaries and channel adapter installation. Nothing is downloaded, installed, removed or repaired by this preview.",
            );
            resource_link(world, parent, Resource::LocalEngine);
            nav_button(
                world,
                parent,
                "settings:4",
                "Sharing & channel configuration",
            );
        }
        7 => {
            band(
                world,
                parent,
                "Gateway & tool servers",
                "Live read-only gateway snapshot below; disconnected servers remain visibly disconnected. No tool invocation, enable/disable, installation or credential change is supported.",
            );
        }
        8 => {
            band(
                world,
                parent,
                "Active workshop",
                "Only an explicitly configured loopback daemon is supported. Launch with --data-dir and --daemon to use the existing local credential boundary; offline mode never discovers credentials.",
            );
            band(
                world,
                parent,
                "Engine & files",
                "Filesystem authority follows the workshop daemon. This port offers no local folder picker, Reveal or remote path conversion. Start, stop, vault linking, workshop switching and transport configuration remain in the existing app.",
            );
            muted(
                world,
                parent,
                "Transport configured does not mean connected. Read health below to verify the responder's authority/version; authentication and service errors remain local to this view.",
            );
        }
        _ => {}
    }
}
