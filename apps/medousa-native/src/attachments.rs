//! Session-owned composer attachments. UI only queues work; picker, disk reads,
//! and authenticated uploads run on the host's async worker, never in Update.
use medousa_sdk::MedousaClient;
use medousa_types::{MediaRef, MediaUploadResponse};
use rndr_core::*;
use rndr_ui::{
    Typography, UiId,
    controls::{self, ButtonProps, HostIntent},
    text,
};
use std::{collections::HashMap, io::Read, path::PathBuf};

const PICK: u64 = 0x4154_5401;
const REMOVE: u64 = 0x4154_5402;
const MAX_BYTES: u64 = 25 * 1024 * 1024;
const MAX_FILES: usize = 10;

#[derive(Default)]
struct Attachments {
    session: Option<String>,
    drafts: HashMap<String, Vec<Item>>,
    commands: Vec<AttachmentCommand>,
    next: u64,
    revision: u64,
}
#[derive(Clone)]
struct Item {
    id: u64,
    label: String,
    state: ItemState,
}
#[derive(Clone)]
enum ItemState {
    Pending,
    Ready(MediaRef),
    Failed(String),
}
#[derive(Clone)]
struct Remove {
    session: String,
    id: u64,
}
impl Component for Remove {}
struct View {
    revision: u64,
    session: Option<String>,
    tray: Entity,
}
impl Component for View {}

/// Opaque request identity is retained from selection through completion.
/// Host must execute every drained command and apply the returned update.
#[derive(Debug)]
pub struct AttachmentCommand {
    pub(crate) session: String,
    id: u64,
}
pub struct AttachmentUpdate {
    session: String,
    id: u64,
    items: Vec<(String, Result<MediaRef, String>)>,
}

pub fn setup(world: &mut World, parent: Entity, tray: Entity) -> Entity {
    if world.resource::<Attachments>().is_none() {
        world.insert_resource(Attachments::default());
    }
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 4.,
            ..Layout::column()
        },
        View {
            revision: u64::MAX,
            session: None,
            tray,
        },
    ));
    set_parent(world, root, parent);
    root
}

pub fn sync(world: &mut World, root: Entity, session: Option<&str>) {
    let Some(state) = world.resource_mut::<Attachments>() else {
        return;
    };
    state.session = session.map(str::to_owned);
    let revision = state.revision;
    let items = session
        .and_then(|s| state.drafts.get(s))
        .cloned()
        .unwrap_or_default();
    let Some(view) = world.get::<View>(root) else {
        return;
    };
    if view.revision == revision && view.session.as_deref() == session {
        return;
    }
    let tray = view.tray;
    for child in children_of(world, root)
        .into_iter()
        .chain(children_of(world, tray))
    {
        despawn(world, child);
    }
    if items.is_empty() {
        world.insert(tray, Hidden);
    } else {
        world.remove::<Hidden>(tray);
    }
    world.get_mut::<Layout>(tray).unwrap().height = Size::Px(
        items
            .iter()
            .map(|i| {
                if matches!(&i.state, ItemState::Ready(m) if m.mime.starts_with("image/")) {
                    96.
                } else {
                    32.
                }
            })
            .sum::<f32>()
            .min(
                if items.iter().any(
                    |i| matches!(&i.state, ItemState::Ready(m) if m.mime.starts_with("image/")),
                ) {
                    220.
                } else {
                    96.
                },
            ),
    );
    let mut props = ButtonProps::new("Attach files", HostIntent::Command(PICK));
    props.variant = controls::ButtonVariant::Quiet;
    props.enabled = session.is_some() && items.len() < MAX_FILES;
    let pick =
        controls::IconButton::icon(world, root, UiId(PICK), Icon::Plus, "Attach files", props)
            .expect("attachment control");
    world.get_mut::<LabelBox>(pick.label).unwrap().inset_x = 0.;
    crate::native_style::action_hint(world, pick.root, "Attach files");
    for item in items {
        let row = world.spawn((Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },));
        set_parent(world, row, tray);
        if let ItemState::Ready(media) = &item.state {
            if media.mime.starts_with("image/") {
                let preview = crate::media_preview::mount(
                    world,
                    row,
                    session.unwrap(),
                    &media.media_id,
                    &item.label,
                    true,
                );
                world.get_mut::<Layout>(preview).unwrap().width = Size::Px(100.);
            }
        }
        let label = match item.state {
            ItemState::Pending => format!("{} — selecting / uploading…", item.label),
            ItemState::Ready(_) => item.label,
            ItemState::Failed(error) => format!("{} — {error}", item.label),
        };
        let label = text(world, row, &label, Typography::Secondary).expect("attachment label");
        world.get_mut::<Layout>(label).unwrap().width = Size::Fill;
        world.get_mut::<Label>(label).unwrap().overflow = TextOverflow::Ellipsis;
        let handle = controls::Button::mount(
            world,
            row,
            UiId(item.id),
            ButtonProps::new("Remove", HostIntent::Command(REMOVE)),
        )
        .expect("remove attachment");
        world.get_mut::<Layout>(handle.root).unwrap().width = Size::Px(76.);
        world.insert(
            handle.root,
            Remove {
                session: session.unwrap().into(),
                id: item.id,
            },
        );
    }
    *world.get_mut::<View>(root).unwrap() = View {
        revision,
        tray,
        session: session.map(str::to_owned),
    };
}

pub fn action(world: &mut World, intent: &ActionIntent) -> bool {
    match controls::host_intent(world, intent) {
        Some(HostIntent::Command(PICK)) => {
            let Some(state) = world.resource_mut::<Attachments>() else {
                return false;
            };
            let Some(session) = state.session.clone() else {
                return true;
            };
            if state.drafts.get(&session).is_some_and(|items| {
                items.len() >= MAX_FILES
                    || items.iter().any(|i| matches!(i.state, ItemState::Pending))
            }) {
                return true;
            }
            state.next += 1;
            let id = state.next;
            state.drafts.entry(session.clone()).or_default().push(Item {
                id,
                label: "Files".into(),
                state: ItemState::Pending,
            });
            state.commands.push(AttachmentCommand { session, id });
            state.revision += 1;
            true
        }
        Some(HostIntent::Command(REMOVE)) => {
            let Some(remove) = world.get::<Remove>(intent.entity).cloned() else {
                return false;
            };
            let state = world.resource_mut::<Attachments>().unwrap();
            if let Some(items) = state.drafts.get_mut(&remove.session) {
                items.retain(|i| i.id != remove.id);
            }
            // An already-running upload is allowed to finish but can never
            // reinsert a removed item or mutate another conversation.
            state
                .commands
                .retain(|c| c.id != remove.id || c.session != remove.session);
            state.revision += 1;
            true
        }
        _ => false,
    }
}

pub fn drain_commands(world: &mut World) -> Vec<AttachmentCommand> {
    world
        .resource_mut::<Attachments>()
        .map(|s| std::mem::take(&mut s.commands))
        .unwrap_or_default()
}

/// Native picker cancellation is successful and removes the pending placeholder.
/// Recoverable failures are carried inside the update so they remain visible and
/// removable in the originating session, including after navigation.
pub async fn execute(
    client: &MedousaClient,
    command: AttachmentCommand,
) -> Result<AttachmentUpdate, String> {
    let mut update = AttachmentUpdate {
        session: command.session,
        id: command.id,
        items: vec![],
    };
    let Some(files) = rfd::AsyncFileDialog::new()
        .set_title("Attach files to Medousa")
        .pick_files()
        .await
    else {
        return Ok(update);
    };
    if files.len() > MAX_FILES {
        update.items.push((
            "Files".into(),
            Err(format!("Select at most {MAX_FILES} files at a time.")),
        ));
        return Ok(update);
    }
    for file in files {
        let path = file.path().to_path_buf();
        let label = file.file_name();
        let result = match tokio::task::spawn_blocking(move || read_file(path)).await {
            Ok(Ok((bytes, mime))) => upload(client, &update.session, &label, &mime, bytes).await,
            Ok(Err(error)) => Err(error),
            Err(error) => Err(format!("Could not read file: {error}")),
        };
        update.items.push((label, result));
    }
    Ok(update)
}

fn read_file(path: PathBuf) -> Result<(Vec<u8>, String), String> {
    let file = std::fs::File::open(&path).map_err(|e| format!("Cannot open file: {e}"))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("Cannot inspect file: {e}"))?;
    if !metadata.is_file() {
        return Err("Only regular files can be attached.".into());
    }
    if metadata.len() == 0 {
        return Err("Empty files cannot be attached.".into());
    }
    if metadata.len() > MAX_BYTES {
        return Err("File exceeds the 25 MiB limit.".into());
    }
    // Bound the actual read too: a file can grow after metadata was read.
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read file: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("File exceeds the 25 MiB limit.".into());
    }
    if bytes.is_empty() {
        return Err("Empty files cannot be attached.".into());
    }
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    Ok((bytes, mime))
}

async fn upload(
    client: &MedousaClient,
    session: &str,
    label: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> Result<MediaRef, String> {
    let path = medousa_sdk::transport::path_with_query(
        medousa_sdk::operations::MEDIA_UPLOAD_POST.path,
        &[("session_id", crate::routing::wire_owned(session))],
    );
    let value = client
        .transport()
        .post_multipart(client.base_url(), &path, label, mime, bytes, Some(label))
        .await
        .map_err(|e| e.to_string())?;
    let response: MediaUploadResponse =
        serde_json::from_value(value).map_err(|e| format!("Invalid media response: {e}"))?;
    if response.media_id.is_empty() {
        return Err("Media upload returned an empty id.".into());
    }
    Ok(MediaRef {
        media_id: response.media_id,
        kind: kind(&response.mime).into(),
        mime: response.mime,
        label: response.label.or_else(|| Some(label.into())),
        source_media_id: None,
        generation_id: None,
        parent_generation_id: None,
    })
}
fn kind(mime: &str) -> &'static str {
    if mime.starts_with("image/") {
        "image"
    } else if mime.starts_with("audio/") {
        "audio"
    } else if mime.contains("spreadsheet") || mime.contains("excel") || mime == "text/csv" {
        "spreadsheet"
    } else {
        "document"
    }
}

pub fn apply(world: &mut World, update: AttachmentUpdate) {
    let Some(state) = world.resource_mut::<Attachments>() else {
        return;
    };
    let Some(items) = state.drafts.get_mut(&update.session) else {
        return;
    };
    let Some(index) = items
        .iter()
        .position(|i| i.id == update.id && matches!(i.state, ItemState::Pending))
    else {
        return;
    };
    items.remove(index);
    for (label, result) in update.items {
        state.next += 1;
        items.push(Item {
            id: state.next,
            label,
            state: match result {
                Ok(media) => ItemState::Ready(media),
                Err(error) => ItemState::Failed(error),
            },
        });
    }
    state.revision += 1;
}

pub fn media_refs(world: &World, session: &str) -> Result<Vec<MediaRef>, String> {
    let Some(items) = world
        .resource::<Attachments>()
        .and_then(|s| s.drafts.get(session))
    else {
        return Ok(vec![]);
    };
    if items.len() > MAX_FILES {
        return Err(format!(
            "Remove attachments to keep at most {MAX_FILES} files per message."
        ));
    }
    items
        .iter()
        .map(|i| match &i.state {
            ItemState::Ready(media) => Ok(media.clone()),
            ItemState::Pending => Err("Wait for attachment uploads before sending.".into()),
            ItemState::Failed(error) => {
                Err(format!("Remove failed attachment '{}': {error}", i.label))
            }
        })
        .collect()
}

/// Clear only the exact references admitted with a successful user turn. New
/// attachments, pending uploads, errors, and other sessions are never cleared.
pub fn clear_admitted(world: &mut World, session: &str, refs: &[MediaRef]) {
    let Some(state) = world.resource_mut::<Attachments>() else {
        return;
    };
    if let Some(items) = state.drafts.get_mut(session) {
        items.retain(|i| !matches!(&i.state, ItemState::Ready(media) if refs.contains(media)));
        state.revision += 1;
    }
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;
