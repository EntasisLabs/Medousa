//! Sole runtime writer of session identity, generations and pending recovery.
use crate::backend::{Command, Reply, Request, Update};
use crate::composer::{SubmitBoundary, reset_prompt};
use crate::shell::Ui;
use crate::{conversation, menus, presentation, shell, state::Model};
use medousa_rndr_home::{Chat, turn_request};
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
            Update::History(session, messages, active) => {
                model.chat = Chat::history(session, active.is_some());
                model.history_messages = messages;
                if let Some(turn) = active {
                    model.chat.resume(turn.turn_id);
                }
                model.loading = false;
            }
            Update::SettledHistory(session, messages) => {
                let status = model
                    .chat
                    .status
                    .trim_end_matches(" · reconciling durable history…")
                    .to_string();
                model.chat = Chat::history(session, false);
                model.history_messages = messages;
                model.chat.status = format!("{status} · history reconciled (non-atomic reads)");
                model.loading = false;
            }
            Update::Mode(mode) => {
                model.mode = Some(mode);
                model.loading = false;
            }
            Update::Modes(modes) => {
                model.modes = modes;
            }
            Update::Admitted(turn) => {
                model.chat.admitted(turn);
                if let Some(index) = model.pending_recovery.take() {
                    model.recovery.remove(index);
                }
            }
            Update::Stream(e) => {
                let terminal = e.event.is_terminal();
                model.chat.apply(e);
                if terminal {
                    model.chat.uncertain = true;
                    model
                        .chat
                        .status
                        .push_str(" · reconciling durable history…");
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
                model.chat.busy = false;
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
        let (busy, loading) = {
            let m = world.resource::<Model>().unwrap();
            (m.chat.busy, m.loading)
        };
        match action.as_str() {
            "new" | "recent" | "reload" if !busy && !loading => {
                let command = match action.as_str() {
                    "new" => Command::New,
                    "recent" => Command::Recent,
                    _ => {
                        let Some(s) = world.resource::<Model>().unwrap().chat.session.clone()
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
                        m.chat.session.clone(),
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
                match m.chat.begin(&prompt) {
                    Ok(()) => {
                        m.generation += 1;
                        m.pending_recovery = Some(m.recovery.len());
                        m.recovery
                            .push((m.chat.session.clone().unwrap(), prompt.clone()));
                        let mut request = turn_request(
                            m.chat.session.as_deref().unwrap(),
                            &prompt,
                            &provider,
                            &model,
                        );
                        request.response_depth_mode = m.depth.clone();
                        menus::close(world);
                        dispatch(world, Command::Send(Box::new(request)));
                        // Prompt emits a snapshot, not a synchronous owner callback.
                        // Never erase edits queued after that submission boundary.
                        if world.resource::<Model>().unwrap().chat.busy
                            && world.get::<Label>(ui.composer).unwrap().text == prompt
                        {
                            reset_prompt(world, ui.composer, "");
                            request_focus(world, ui.composer);
                        }
                    }
                    Err(e) => m.chat.status = e.into(),
                }
            }
            "recover"
                if !busy
                    && !loading
                    && world.get::<Label>(ui.composer).unwrap().text.is_empty() =>
            {
                let m = world.resource_mut::<Model>().unwrap();
                if let Some((_, prompt)) = m.recovery.pop() {
                    m.pending_recovery = None;
                    reset_prompt(world, ui.composer, &prompt);
                    request_focus(world, ui.composer);
                }
            }
            "cancel" if !loading && (busy || world.resource::<Model>().unwrap().chat.uncertain) => {
                if let Some(id) = world.resource::<Model>().unwrap().chat.session.clone() {
                    dispatch(world, Command::Cancel(id));
                }
            }
            _ => {}
        }
    }
    shell::sync(world);
    #[cfg(feature = "observe")]
    crate::fixtures::sync_observation(world);
}
