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
}
enum Update {
    Note(crate::notes::NoteUpdate),
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
    tx: mpsc::Sender<Update>,
    wake: feed::Sender<AuxiliaryWake>,
}
impl Updates {
    fn send(&self, update: Update) -> Result<(), mpsc::SendError<Update>> {
        self.tx.send(update)?;
        let _ = self.wake.try_send(AuxiliaryWake);
        Ok(())
    }
}
pub struct Auxiliary {
    tx: tokio::sync::mpsc::UnboundedSender<Command>,
    rx: Mutex<mpsc::Receiver<Update>>,
    pub error: Option<String>,
    _wake: Feed<AuxiliaryWake>,
}
impl Auxiliary {
    pub fn start(client: MedousaClient, world: &mut World) -> Self {
        let client = Arc::new(client);
        let previews = Arc::new(tokio::sync::Semaphore::new(3));
        let (tx, mut commands) = tokio::sync::mpsc::unbounded_channel();
        let (tx_updates, rx) = mpsc::channel();
        let (wake, sender) = feed::open(world, FeedConfig::latest());
        let updates = Updates {
            tx: tx_updates,
            wake: sender,
        };
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("native action runtime");
            runtime.block_on(async move { while let Some(command)=commands.recv().await {
                let client=client.clone(); let updates=updates.clone(); let previews=previews.clone();
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
                    Command::Note(c)=>Ok(Update::Note(crate::notes::execute(&client,c).await)),
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
        let _ = self.tx.send(command);
    }
}
pub fn tick(world: &mut World) {
    let updates = world
        .resource::<Auxiliary>()
        .map(|a| a.rx.lock().unwrap().try_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    for update in updates {
        match update {
            Update::Note(u) => crate::notes::apply(world, u),
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
            Update::Error(error) => world.resource_mut::<Auxiliary>().unwrap().error = Some(error),
        }
    }
}

// Run after UI reconciliation: newly mounted previews and button actions must
// dispatch before the event-driven window parks, not on a future user event.
pub fn dispatch(world: &mut World) {
    media_preview::tick(world);
    let commands = crate::notes::drain_commands(world)
        .into_iter()
        .map(Command::Note)
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
    if let Some(aux) = world.resource::<Auxiliary>() {
        for command in commands {
            aux.send(command);
        }
    }
}
