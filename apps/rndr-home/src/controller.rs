//! Host connection generation fencing and canonical core command dispatch.
use crate::backend::{Command, Reply, Request, Update};
use crate::composer::SubmitBoundary;
use crate::shell::Ui;
use crate::{conversation, menus, presentation, shell, state::Model};
use medousa_app_core::TurnFact;
use medousa_rndr_home::turn_request;
use rndr_core::*;
use rndr_ui::{Appearance, Contrast};

fn dispatch(world: &mut World, command: Command) {
    let model = world.resource_mut::<Model>().unwrap();
    if let Some(tx) = &model.commands {
        if tx
            .send(Request {
                generation: model.generation,
                command,
            })
            .is_err()
        {
            model
                .chat
                .fail("Connection worker unavailable. Restart after fixing daemon/auth.".into());
            model.loading = false;
            model.commands = None;
        }
    } else {
        model.chat.status = "Offline preview · restart with --data-dir to connect".into();
        model.loading = false;
    }
}
pub(super) fn update(world: &mut World) {
    let ui = *world.resource::<Ui>().unwrap();
    let replies = events::<Reply>(world).to_vec();
    for reply in replies {
        let model = world.resource_mut::<Model>().unwrap();
        if reply.generation != model.generation
            && !matches!(
                reply.update,
                Update::Models(_) | Update::Modes(_) | Update::Unavailable(_)
            )
        {
            continue;
        }
        match reply.update {
            Update::History(session, authority, turns, active) => {
                let active = active.map(|turn| TurnFact {
                    turn_id: turn.turn_id,
                    mode: "interactive".into(),
                    phase: "streaming".into(),
                    message_id: None,
                    workspace_card_id: None,
                    reusable_stream: true,
                    budget_request_id: None,
                    requested_rounds: None,
                });
                if let Err(e) = model.chat.hydrate(session, authority, turns, active) {
                    model.chat.fail(e);
                }
                model.loading = false;
            }
            Update::SettledHistory(session, authority, turns) => {
                if let Err(e) = model.chat.hydrate(session, authority, turns, None) {
                    model.chat.fail(e);
                }
                model.loading = false;
            }
            Update::Mode(mode) => {
                model.mode = Some(mode);
                model.loading = false;
            }
            Update::Modes(modes) => {
                model.modes = modes;
            }
            Update::Admitted(turn, prompt) => {
                if let Err(e) = model.chat.admitted(turn, prompt) {
                    model.chat.fail(e);
                }
                model.loading = false;
            }
            Update::Stream(e) => {
                let terminal = e.event.is_terminal();
                if let Err(e) = model.chat.apply(e) {
                    model.chat.fail(e);
                }
                if terminal {
                    model.chat.uncertain = true;
                    model.chat.status = "Reconciling durable history…".into();
                }
            }
            Update::Models(models) => {
                model.models = models;
                model.chat.status =
                    "Connected · create + or continue a chat; select model before sending".into();
            }
            Update::Notice(message) => {
                model.chat.status = message;
                model.loading = false;
            }
            Update::Failure(message) => {
                model.chat.fail(message);
                model.loading = false;
            }
            Update::Unavailable(message) => {
                model.chat.status = message;
                model.loading = false;
                model.commands = None;
            }
        }
    }
    let mut actions = take_action_intents(world)
        .into_iter()
        .map(|a| {
            let action = presentation::appearance_action(world, &a)
                .map(str::to_owned)
                .unwrap_or(a.name);
            (action, None)
        })
        .collect::<Vec<_>>();
    if world.resource::<SubmitBoundary>().unwrap().safe {
        actions.extend(
            events::<Submitted>(world)
                .iter()
                .filter(|s| s.entity == ui.composer)
                .map(|s| ("send".into(), Some(s.text.clone()))),
        );
    } else if events::<Submitted>(world)
        .iter()
        .any(|s| s.entity == ui.composer)
    {
        world.resource_mut::<Model>().unwrap().chat.status = "Input changed in the Enter batch; nothing sent, draft preserved. Press Send or Enter again after composition finishes.".into();
    }
    for (action, submitted) in actions {
        if crate::desktop::action(world, &action) {
            continue;
        }
        let (busy, loading) = {
            let m = world.resource::<Model>().unwrap();
            (m.chat.busy(), m.loading)
        };
        // UI disabling is not admission control: reject forged/queued intents too.
        if matches!(
            action.as_str(),
            "send" | "recover" | "new" | "recent" | "reload"
        ) && !crate::composer::native_input_ready(world)
        {
            world.resource_mut::<Model>().unwrap().chat.status =
                "Native input safety pending · draft preserved; action disabled".into();
            continue;
        }
        match action.as_str() {
            "history:latest" => {
                if conversation::jump_to_latest(world, ui.conversation)
                    && world.resource::<Focus>().and_then(|f| f.entity) == Some(ui.latest)
                {
                    conversation::focus_history(world, ui.conversation);
                }
            }
            "new" | "recent" | "reload" if !busy && !loading => {
                let command = match action.as_str() {
                    "new" => Command::New,
                    "recent" => Command::Recent,
                    _ => {
                        let Some(s) = world
                            .resource::<Model>()
                            .unwrap()
                            .chat
                            .session()
                            .map(str::to_string)
                        else {
                            continue;
                        };
                        Command::Open(s)
                    }
                };
                let m = world.resource_mut::<Model>().unwrap();
                m.loading = true;
                m.mode = None;
                m.generation += 1;
                menus::close(world);
                dispatch(world, command);
            }
            "model" if !busy => menus::open(world, menus::Kind::Model, ui.model),
            "runtime" if !busy => menus::open(world, menus::Kind::Runtime, ui.runtime),
            "mode" if !busy && !loading => menus::open(world, menus::Kind::Mode, ui.mode),
            "behavior" if !busy => menus::open(world, menus::Kind::Behavior, ui.behavior),
            _ if action.starts_with("copy-message:") => {
                conversation::copy_message(world, ui.conversation, &action);
            }
            "menu:close" => menus::close(world),
            "appearance:open" if !busy => menus::open(world, menus::Kind::Appearance, ui.behavior),
            "appearance:light" | "appearance:dark" | "appearance:contrast" => {
                let mut theme = presentation::theme(world);
                match action.as_str() {
                    "appearance:light" => theme.appearance = Appearance::Light,
                    "appearance:dark" => theme.appearance = Appearance::Dark,
                    _ => {
                        theme.contrast = if theme.contrast == Contrast::High {
                            Contrast::Standard
                        } else {
                            Contrast::High
                        }
                    }
                }
                presentation::apply_theme(world, theme);
            }
            "behavior:depth" if !busy => menus::open(world, menus::Kind::Depth, ui.behavior),
            "behavior:main" if !busy => menus::open(world, menus::Kind::Behavior, ui.behavior),
            _ if action.starts_with("pick:") && !busy => {
                if let Ok(index) = action[5..].parse::<usize>() {
                    let m = world.resource_mut::<Model>().unwrap();
                    if index < m.models.len() {
                        m.selection = Some(index);
                    }
                }
                menus::close(world);
            }
            _ if action.starts_with("depth:") && !busy => {
                let depth = &action[6..];
                if ["concise", "standard", "deep"].contains(&depth) {
                    world.resource_mut::<Model>().unwrap().depth = depth.into();
                }
                menus::open(world, menus::Kind::Behavior, ui.behavior);
            }
            _ if action.starts_with("mode:") && !busy && !loading => {
                let id = &action[5..];
                let m = world.resource::<Model>().unwrap();
                if m.modes.iter().any(|(mode, _, ok)| mode == id && *ok)
                    && let (Some(session), Ok(mode)) = (
                        m.chat.session().map(str::to_string),
                        serde_json::from_value(serde_json::Value::String(id.into())),
                    )
                {
                    menus::close(world);
                    world.resource_mut::<Model>().unwrap().loading = true;
                    dispatch(world, Command::SetMode(session, mode));
                }
            }
            "send" if !loading => {
                let prompt = submitted
                    .unwrap_or_else(|| world.get::<Label>(ui.composer).unwrap().text.clone());
                let m = world.resource_mut::<Model>().unwrap();
                if m.commands.is_none() {
                    m.chat.status = "Offline preview · nothing sent".into();
                    continue;
                }
                let Some(index) = m.selection else {
                    m.chat.status =
                        "Choose a model from the menu first; sending may incur charges".into();
                    continue;
                };
                let (provider, model) = m.models[index].clone();
                if prompt.trim().is_empty()
                    || m.chat.busy()
                    || m.chat.uncertain
                    || m.chat.session().is_none()
                {
                    m.chat.status =
                        "Open/reconcile a session before sending a nonempty prompt".into();
                    continue;
                }
                m.generation += 1;
                m.loading = true;
                let mut request =
                    turn_request(m.chat.session().unwrap(), &prompt, &provider, &model);
                request.response_depth_mode = m.depth.clone();
                menus::close(world);
                dispatch(world, Command::Send(Box::new(request)));
                // No optimistic execution/recovery state. Draft stays intact if
                // admission fails; guarded input actions remain denied.
            }
            "cancel" if !loading && (busy || world.resource::<Model>().unwrap().chat.uncertain) => {
                if let Some(id) = world
                    .resource::<Model>()
                    .unwrap()
                    .chat
                    .session()
                    .map(str::to_string)
                {
                    dispatch(world, Command::Cancel(id));
                }
            }
            _ => {}
        }
    }
    shell::sync(world);
    crate::desktop::sync(world);
    #[cfg(feature = "observe")]
    crate::fixtures::sync_observation(world);
}
