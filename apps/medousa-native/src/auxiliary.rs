//! Off-frame workers for scoped native actions. Mutation failures are reported,
//! never retried automatically. UI reconciliation remains on the app thread.
use crate::{approvals, attachments, context, media_preview, runtimes};
use medousa_sdk::MedousaClient;
use rndr_core::*;
use std::sync::{Arc, Mutex, mpsc};

pub enum Command {
    Preview(media_preview::Key),
    Context(context::ContextCommand),
    Approval(approvals::ApprovalCommand),
    Attachment(attachments::AttachmentCommand),
    Runtime(runtimes::RuntimeCommand),
    Stream(runtimes::RuntimeStream),
    Note(crate::notes::NoteCommand),
    Code(crate::code::CodeCommand),
    Review(crate::reviews::Command),
    Calendar(crate::calendar::Command),
    Create(crate::create::Command),
    Settings(crate::settings::Command),
    Peers(crate::peers::Command),
    PeerManagement(crate::peer_management::Command),
    Workshop(crate::ui::workshop_status::Command),
}
#[derive(Clone)]
struct Stamp {
    view: crate::surfaces::SurfaceId,
    binding: crate::routing::Binding,
}
enum Update {
    Note(crate::notes::NoteUpdate),
    Code(crate::code::CodeUpdate),
    Review(crate::reviews::Update),
    Calendar(crate::calendar::Update),
    Create(crate::create::Update),
    Settings(crate::settings::Update),
    Peers(crate::peers::Update),
    PeerManagement(crate::peer_management::Update),
    Workshop(crate::ui::workshop_status::Update),
    Preview(media_preview::PreviewUpdate),
    Context(context::ContextUpdate),
    Approval(approvals::ApprovalUpdate),
    Attachment(attachments::AttachmentUpdate),
    Runtime(runtimes::RuntimeUpdate),
    RuntimeEvent(
        String,
        String,
        Result<medousa_app_core::ChatCommand, String>,
    ),
    Error(String),
}
// mpsc carries owned payloads; a coalescing RNDR feed wakes the parked window.
// Sending without this wake left completed images/loading actions invisible
// until an unrelated pointer or keyboard event arrived.
struct AuxiliaryWake;
impl Event for AuxiliaryWake {}
#[derive(Clone)]
struct Updates {
    tx: mpsc::Sender<(Option<Stamp>, Update)>,
    wake: feed::Sender<AuxiliaryWake>,
    stamp: Option<Stamp>,
}
impl Updates {
    fn send(&self, update: Update) -> Result<(), ()> {
        self.tx.send((self.stamp.clone(), update)).map_err(|_| ())?;
        let _ = self.wake.try_send(AuxiliaryWake);
        Ok(())
    }
}
pub struct Auxiliary {
    tx: tokio::sync::mpsc::UnboundedSender<(Option<Stamp>, Command)>,
    rx: Mutex<mpsc::Receiver<(Option<Stamp>, Update)>>,
    pub error: Option<String>,
    _wake: Feed<AuxiliaryWake>,
}
impl Auxiliary {
    pub fn start(client: Arc<MedousaClient>, world: &mut World) -> Self {
        let previews = Arc::new(tokio::sync::Semaphore::new(3));
        let (tx, mut commands) = tokio::sync::mpsc::unbounded_channel();
        let (tx_updates, rx) = mpsc::channel();
        let (wake, sender) = feed::open(world, FeedConfig::latest());
        let updates = Updates {
            tx: tx_updates,
            wake: sender,
            stamp: None,
        };
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("native action runtime");
            runtime.block_on(async move { while let Some((stamp,command))=commands.recv().await {
                let client=client.clone(); let mut updates=updates.clone(); updates.stamp=stamp; let previews=previews.clone();
                tokio::spawn(async move {
                    if let Command::Stream(source)=command {
                        let session=source.session.clone();let agent=source.agent_session_id.clone();
                        let (tx,mut rx)=tokio::sync::mpsc::unbounded_channel();
                        tokio::spawn(async move {runtimes::stream(&client,source,tx).await;});
                        while let Some(event)=rx.recv().await {let terminal=matches!(&event,Ok(medousa_app_core::ChatCommand::ApplyStreamEvent{envelope,..}) if envelope.event.is_terminal());let failed=event.is_err();let _=updates.send(Update::RuntimeEvent(session.clone(),agent.clone(),event));if terminal||failed{break;}}
                        return;
                    }
                    let result=match command {
                    Command::Preview(c)=> { let _permit=previews.acquire_owned().await.expect("preview semaphore"); Ok(Update::Preview(media_preview::execute(&client,c).await)) },
                    Command::PeerManagement(c)=>Ok(Update::PeerManagement(crate::peer_management::execute(&client,c).await)),
                    Command::Peers(c)=>Ok(Update::Peers(crate::peers::execute(&client,c).await)),
                    Command::Workshop(c)=>Ok(Update::Workshop(crate::ui::workshop_status::execute(&client,c).await)),
                    Command::Calendar(c)=>Ok(Update::Calendar(crate::calendar::execute(&client,c).await)),
                    Command::Create(c)=>Ok(Update::Create(crate::create::execute(&client,c).await)),
                    Command::Settings(c)=>Ok(Update::Settings(crate::settings::execute(&client,c).await)),
                    Command::Note(c)=>Ok(Update::Note(crate::notes::execute(&client,c).await)),
                    Command::Review(c)=>Ok(Update::Review(crate::reviews::execute(&client,c).await)),
                    Command::Code(c)=>Ok(Update::Code(crate::code::execute(&client,c).await)),
                    Command::Context(c)=>context::execute(&client,c).await.map(Update::Context),
                    Command::Approval(c)=>approvals::execute(&client,c).await.map(Update::Approval),
                    Command::Attachment(c)=>attachments::execute(&client,c).await.map(Update::Attachment),
                    Command::Runtime(c)=>runtimes::execute(&client,c).await.map(Update::Runtime),
                    Command::Stream(_)=>unreachable!(),
                }; let _=updates.send(result.unwrap_or_else(Update::Error)); });
            }});
        });
        Self {
            tx,
            rx: Mutex::new(rx),
            error: None,
            _wake: wake,
        }
    }
    pub fn send(&self, command: Command) {
        let _ = self.tx.send((None, command));
    }
}
pub fn tick(world: &mut World) {
    let updates: Vec<_> = if let Some(s) = world.resource::<crate::workshops::Workshops>() {
        s.auxiliaries
            .values()
            .flat_map(|aux| aux.rx.lock().unwrap().try_iter().collect::<Vec<_>>())
            .collect()
    } else {
        world
            .resource::<Auxiliary>()
            .map(|aux| aux.rx.lock().unwrap().try_iter().collect())
            .unwrap_or_default()
    };
    for (stamp, update) in updates {
        if let Some(stamp) = stamp {
            if !crate::workshops::binding(world, stamp.view)
                .is_some_and(|b| b.accepts(&stamp.binding))
            {
                continue;
            }
        }
        match update {
            Update::PeerManagement(u) => crate::peer_management::apply(world, u),
            Update::Peers(u) => crate::peers::apply(world, u),
            Update::Workshop(u) => crate::ui::workshop_status::apply(world, u),
            Update::Calendar(u) => crate::calendar::apply(world, u),
            Update::Create(u) => crate::create::apply(world, u),
            Update::Settings(u) => crate::settings::apply(world, u),
            Update::Note(u) => crate::notes::apply(world, u),
            Update::Review(u) => crate::reviews::apply(world, u),
            Update::Code(u) => crate::code::apply(world, u),
            Update::Preview(u) => media_preview::apply(world, u),
            Update::Context(u) => context::apply(world, u),
            Update::Approval(u) => approvals::apply(world, u),
            Update::Attachment(u) => attachments::apply(world, u),
            Update::Runtime(u) => {
                if let Some(session) = runtimes::admitted(&u) {
                    crate::ui::external_admitted(world, session);
                }
                runtimes::apply(world, u);
            }
            Update::RuntimeEvent(session, agent, event) => match event {
                Ok(command) => {
                    let terminal = matches!(&command,medousa_app_core::ChatCommand::ApplyStreamEvent{envelope,..} if envelope.event.is_terminal());
                    crate::ui::runtime_event(world, &session, command);
                    if terminal {
                        runtimes::settled(world, &session, &agent);
                    }
                }
                Err(error) => {
                    runtimes::stream_lost(world, &session, &agent);
                    crate::ui::runtime_error(world, &session, error);
                }
            },
            Update::Error(error) => {
                if let Some(aux) = world.resource_mut::<Auxiliary>() {
                    aux.error = Some(error);
                }
            }
        }
    }
}

// Run after UI reconciliation: newly mounted previews and button actions must
// dispatch before the event-driven window parks, not on a future user event.
fn available(world: &World, view: crate::surfaces::SurfaceId) -> bool {
    world
        .resource::<crate::workshops::Workshops>()
        .is_none_or(|s| {
            s.bindings
                .get(&view)
                .is_some_and(|b| s.auxiliaries.contains_key(&b.authority))
        })
}
pub fn dispatch(world: &mut World) {
    if world
        .resource::<crate::workshops::Workshops>()
        .is_some_and(|s| s.auxiliaries.is_empty())
    {
        return;
    }
    media_preview::tick(world);
    let commands = (if available(world, crate::surfaces::SurfaceId::Notes) {
        crate::notes::drain_commands(world)
    } else {
        vec![]
    })
    .into_iter()
    .map(Command::Note)
    .chain(
        (if available(world, crate::surfaces::SurfaceId::Calendar) {
            crate::calendar::take_commands(world)
        } else {
            vec![]
        })
        .into_iter()
        .map(Command::Calendar),
    )
    .chain(
        (if available(world, crate::surfaces::SurfaceId::Settings) {
            crate::settings::take_commands(world)
        } else {
            vec![]
        })
        .into_iter()
        .map(Command::Settings),
    )
    .chain(
        (if available(world, crate::surfaces::SurfaceId::Create) {
            crate::create::take_commands(world)
        } else {
            vec![]
        })
        .into_iter()
        .map(Command::Create),
    )
    .chain(
        (if available(world, crate::surfaces::SurfaceId::Peers) {
            crate::peers::take_commands(world)
        } else {
            vec![]
        })
        .into_iter()
        .map(Command::Peers),
    )
    .chain(
        (if available(world, crate::surfaces::SurfaceId::Settings) {
            crate::ui::workshop_status::take_commands(world)
        } else {
            vec![]
        })
        .into_iter()
        .map(Command::Workshop),
    )
    .chain(
        (if available(world, crate::surfaces::SurfaceId::Peers) {
            crate::peer_management::take_commands(world)
        } else {
            vec![]
        })
        .into_iter()
        .map(Command::PeerManagement),
    )
    .chain(
        (if available(world, crate::surfaces::SurfaceId::Reviews) {
            crate::reviews::take_commands(world)
        } else {
            vec![]
        })
        .into_iter()
        .map(Command::Review),
    )
    .chain(
        (if available(world, crate::surfaces::SurfaceId::Code) {
            crate::code::take_commands(world)
        } else {
            vec![]
        })
        .into_iter()
        .map(Command::Code),
    )
    .chain(
        media_preview::drain_commands(world)
            .into_iter()
            .map(Command::Preview),
    )
    .chain(
        context::drain_commands(world)
            .into_iter()
            .map(Command::Context),
    )
    .chain(
        approvals::drain_commands(world)
            .into_iter()
            .map(Command::Approval),
    )
    .chain(
        attachments::drain_commands(world)
            .into_iter()
            .map(Command::Attachment),
    )
    .chain(
        runtimes::drain_commands(world)
            .into_iter()
            .map(Command::Runtime),
    )
    .chain(
        runtimes::drain_streams(world)
            .into_iter()
            .map(Command::Stream),
    )
    .collect::<Vec<_>>();
    for command in commands {
        send(world, command);
    }
}
/// Capture one session owner or one view's immutable binding for this request.
pub(crate) fn send(world: &World, command: Command) {
    use crate::surfaces::SurfaceId as V;
    if let Some(s) = world.resource::<crate::workshops::Workshops>() {
        let view = match &command {
            Command::Note(_) => Some(V::Notes),
            Command::Create(_) => Some(V::Create),
            Command::Code(_) => Some(V::Code),
            Command::Review(_) => Some(V::Reviews),
            Command::Calendar(_) => Some(V::Calendar),
            Command::Settings(_) | Command::Workshop(_) => Some(V::Settings),
            Command::Peers(_) | Command::PeerManagement(_) => Some(V::Peers),
            _ => None,
        };
        let (owner, stamp) = if let Some(view) = view {
            let Some(binding) = s.bindings.get(&view).cloned() else {
                return;
            };
            (binding.authority.clone(), Some(Stamp { view, binding }))
        } else {
            let session = match &command {
                Command::Preview(c) => &c.session,
                Command::Attachment(c) => &c.session,
                Command::Context(c) => &c.session,
                Command::Approval(c) => &c.session,
                Command::Runtime(c) => &c.session,
                Command::Stream(c) => &c.session,
                _ => unreachable!(),
            };
            let Some(key) = crate::routing::SessionKey::parse(session) else {
                return;
            };
            (key.authority, None)
        };
        if let Some(aux) = s.auxiliaries.get(&owner) {
            let _ = aux.tx.send((stamp, command));
        }
    } else if let Some(aux) = world.resource::<Auxiliary>() {
        aux.send(command);
    }
}
