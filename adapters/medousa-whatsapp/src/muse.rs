//! Muse's WASA envelope, as used by WAWebWasaHatchOutboundWrapper.
//!
//! Signal still encrypts each device's copy. Only the bot copy gets the inner
//! WASA wrapper; companions receive the normal DeviceSentMessage. Root secrets
//! come from authenticated WhatsApp app-state, never from browser storage.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, ensure};
use async_trait::async_trait;
use buffa::MessageField;
use prost::Message as _;
use rand::RngExt;
use tokio::sync::Mutex;
use wacore::appstate::hash::HashState;
use wacore::appstate::patch_decode::parse_patch_lists_ref;
use wacore::appstate::{self, AppStateError, ExpandedAppStateKeys};
use wacore::libsignal::crypto::{aes_256_cbc_decrypt_into, aes_256_gcm_encrypt};
use wacore::messages::MessageUtils;
use wacore::msg_secret::OriginalMessageResolver;
use wacore::proto_helpers::MessageExt;
use wacore::types::message::{BotEditType, MsgBotInfo};
use wacore_binary::builder::NodeBuilder;
use wacore_binary::{Jid, NodeContent};
use waproto::whatsapp as wa;
use whatsapp_rust::Client;
use whatsapp_rust::NodeFilter;
use whatsapp_rust::request::{InfoQuery, InfoQueryType};

type SyncKeys = HashMap<Vec<u8>, Arc<ExpandedAppStateKeys>>;

// Public service identity from WAWebBotUtils, not an account identifier.
const MUSE_JID: &str = "1807055946647697@bot";
const COLLECTIONS: [&str; 5] = [
    "critical_block",
    "critical_unblock_low",
    "regular_low",
    "regular_high",
    "regular",
];

#[derive(Default, Clone)]
struct Collection {
    hash: HashState,
    macs: HashMap<Vec<u8>, Vec<u8>>,
}

// Deliberately no Debug/Serialize: these values must never enter diagnostics.
#[derive(Default, Clone)]
struct SyncState {
    collections: HashMap<String, Collection>,
    roots: HashMap<String, [u8; 32]>,
    active: Option<String>,
    root_collection: Option<String>,
}

#[derive(Default)]
pub struct MuseProtocol {
    client: OnceLock<Weak<Client>>,
    state: Mutex<SyncState>,
    send_lock: Mutex<()>,
}

pub fn is_muse(jid: &Jid) -> bool {
    jid.to_non_ad().to_string() == MUSE_JID
}

/// The daemon's inbound route is an idempotent message journal, not a token
/// stream. Only publish the final replacement: storing the first preview makes
/// its message ID immutable and can freeze the conversation at that fragment.
/// Use the protocol's terminal marker, never an inactivity timeout.
pub fn completed_message_text(message: &wa::Message, bot: Option<&MsgBotInfo>) -> Option<String> {
    if matches!(
        bot.and_then(|b| b.edit_type),
        Some(BotEditType::First | BotEditType::Inner)
    ) {
        return None;
    }
    message_text(message)
}

fn message_text(message: &wa::Message) -> Option<String> {
    // Streaming replacements arrive inside ProtocolMessage.editedMessage.
    // wacore's get_base_message only peels the separate FutureProof wrapper.
    let message = reply_body(message)?;
    if let Some(text) = message.text_content() {
        return Some(text.to_string());
    }
    let rich = message
        .get_base_message()
        .rich_response_message
        .as_option()?;
    if let Some(data) = rich.unified_response.data.as_deref()
        && let Some(text) = unified_response_text(data)
    {
        return Some(text);
    }
    let parts = rich
        .submessages
        .iter()
        .filter_map(|part| part.message_text.as_deref())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

fn reply_body(mut message: &wa::Message) -> Option<&wa::Message> {
    for _ in 0..8 {
        message = message.get_base_message();
        let Some(protocol) = message.protocol_message.as_option() else {
            return Some(message);
        };
        if protocol.r#type != Some(wa::message::protocol_message::Type::MESSAGE_EDIT) {
            return None;
        }
        message = protocol.edited_message.as_option()?;
    }
    None
}

/// Mirrors getPlainTextFromUnifiedResponse's explicit layout/primitive allowlist.
/// Do not recursively collect arbitrary JSON strings: they include private
/// metadata, URLs, and rendering instructions that are not conversation text.
fn unified_response_text(data: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(data).ok()?;
    let mut parts = Vec::new();
    for section in value.get("sections")?.as_array()? {
        let Some(view) = section.get("view_model") else {
            continue;
        };
        let primitives = match view.get("__typename").and_then(|v| v.as_str()) {
            Some("GenAISingleLayoutViewModel") => {
                view.get("primitive").into_iter().collect::<Vec<_>>()
            }
            Some("GenAIGridLayoutViewModel" | "GenAIHScrollLayoutViewModel") => view
                .get("primitives")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        for primitive in primitives {
            let text = match primitive.get("__typename").and_then(|v| v.as_str()) {
                Some("GenAIMarkdownTextUXPrimitive" | "FOATextPrimitive") => primitive
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(strip_rich_tags),
                Some("GenAIMetadataTextPrimitive") => primitive
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                Some(
                    "GenAIBotProgressStatusPrimitive"
                    | "GenAIBotThinkingStatusPrimitive"
                    | "GenAIProductItemCardPrimitive",
                ) => primitive
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                Some("GenAICodeUXPrimitive") => primitive
                    .get("code_blocks")
                    .and_then(|v| v.as_array())
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter_map(|b| b.get("content")?.as_str())
                            .collect::<String>()
                    }),
                Some("GenATableUXPrimitive") => primitive
                    .get("rows")
                    .and_then(|v| v.as_array())
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|r| r.get("cells")?.as_array())
                            .map(|cells| {
                                cells
                                    .iter()
                                    .filter_map(|c| c.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" | ")
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    }),
                Some("GenAILatexUXPrimitive") => primitive
                    .get("item")
                    .unwrap_or(primitive)
                    .get("latex_expression")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                _ => None,
            };
            if let Some(text) = text.filter(|s| !s.trim().is_empty()) {
                parts.push(text);
            }
        }
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn strip_rich_tags(mut text: &str) -> String {
    let mut plain = String::new();
    while let Some(start) = text.find("{{") {
        plain.push_str(&text[..start]);
        text = &text[start..];
        if let Some(end) = text.find("}}") {
            let name = &text[2..end];
            if !name.is_empty() && !name.chars().any(|c| c.is_whitespace() || c == '}') {
                let closing = format!("{{{{/{name}}}}}");
                let content = &text[end + 2..];
                if let Some(close) = content.find(&closing) {
                    plain.push_str(&content[..close]);
                    text = &content[close + closing.len()..];
                    continue;
                }
            }
        }
        plain.push_str("{{");
        text = &text[2..];
    }
    plain.push_str(text);
    plain
}

impl MuseProtocol {
    pub fn attach(&self, client: &Arc<Client>) {
        let _ = self.client.set(Arc::downgrade(client));
    }

    pub async fn reset(&self) {
        *self.state.lock().await = SyncState::default();
    }

    /// A separate in-memory app-state cursor avoids changing the library's
    /// persisted cursor or replaying contact/chat settings into the application.
    pub async fn refresh(&self) -> Result<()> {
        let client = self
            .client
            .get()
            .and_then(Weak::upgrade)
            .context("Muse client unavailable")?;
        let mut guard = self.state.lock().await;
        let result = Self::refresh_locked(&client, &mut guard).await;
        if result.is_err() {
            // A diverged cursor must not wedge every subsequent send. Retry
            // from authenticated snapshots next time; retain old receive keys.
            guard.collections.clear();
        }
        result
    }

    async fn refresh_locked(client: &Arc<Client>, guard: &mut SyncState) -> Result<()> {
        let mut next = guard.clone();
        let backend = client.persistence_manager().backend();
        let mut pending = COLLECTIONS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        for _ in 0..500 {
            let nodes: Vec<_> = pending
                .iter()
                .map(|name| {
                    let version = next.collections.get(name).map_or(0, |s| s.hash.version);
                    let builder = NodeBuilder::new("collection")
                        .attr("name", name.as_str())
                        .attr(
                            "return_snapshot",
                            if version == 0 { "true" } else { "false" },
                        );
                    if version == 0 {
                        builder.build()
                    } else {
                        builder.attr("version", version).build()
                    }
                })
                .collect();
            let response = client
                .send_iq(InfoQuery {
                    namespace: "w:sync:app:state",
                    query_type: InfoQueryType::Set,
                    to: "s.whatsapp.net".parse()?,
                    target: None,
                    id: None,
                    content: Some(NodeContent::Nodes(vec![
                        NodeBuilder::new("sync").children(nodes).build(),
                    ])),
                    timeout: Some(Duration::from_secs(30)),
                })
                .await
                .map_err(|_| anyhow!("Muse key sync request failed"))?;
            let lists = parse_patch_lists_ref(response.get())
                .map_err(|_| anyhow!("invalid Muse key sync response"))?;
            ensure!(
                lists.len() == pending.len(),
                "incomplete Muse key sync response"
            );
            let mut seen = HashSet::new();
            let mut more = Vec::new();
            for mut list in lists {
                let name = list.name.as_str().to_string();
                ensure!(
                    pending.contains(&name),
                    "unexpected Muse key sync collection"
                );
                ensure!(
                    seen.insert(name.clone()),
                    "duplicate Muse key sync collection"
                );
                ensure!(list.error.is_none(), "Muse key sync collection rejected");
                if let Some(reference) = &list.snapshot_ref {
                    let bytes = client
                        .download(reference)
                        .await
                        .map_err(|_| anyhow!("Muse key snapshot download failed"))?;
                    list.snapshot = Some(
                        waproto::codec::syncd_snapshot_decode(&bytes)
                            .context("decode Muse key snapshot")?,
                    );
                }
                for patch in &mut list.patches {
                    if let Some(reference) = patch.external_mutations.as_option() {
                        let bytes = client
                            .download(reference)
                            .await
                            .map_err(|_| anyhow!("Muse key patch download failed"))?;
                        patch.mutations = waproto::codec::syncd_mutations_decode(&bytes)
                            .context("decode Muse key patch")?
                            .mutations;
                    }
                }
                let mut keys = HashMap::new();
                let key_ids = appstate::collect_key_id_refs_from_patch_list(
                    list.snapshot.as_ref(),
                    &list.patches,
                );
                let mut missing_keys = false;
                for id in &key_ids {
                    missing_keys |= backend.get_sync_key(id).await?.is_none();
                }
                if missing_keys {
                    // Let the normal sync worker request missing keys from our
                    // own devices, with its normal authentication and retry path.
                    client
                        .process_sync_task(whatsapp_rust::sync_task::MajorSyncTask::AppStateSync {
                            name: list.name,
                            full_sync: true,
                        })
                        .await;
                }
                for id in key_ids {
                    let key = backend
                        .get_sync_key(id)
                        .await?
                        .context("WhatsApp app-state keys are not ready for Muse")?;
                    keys.insert(
                        id.to_vec(),
                        Arc::new(appstate::expand_app_state_keys(&key.key_data)),
                    );
                }
                let get_keys = |id: &[u8]| keys.get(id).cloned().ok_or(AppStateError::KeyNotFound);
                let mut collection = next.collections.get(&name).cloned().unwrap_or_default();
                if let Some(snapshot) = &list.snapshot {
                    let result = appstate::process_snapshot(
                        snapshot,
                        &mut HashState::default(),
                        get_keys,
                        true,
                        &name,
                    )
                    .map_err(|_| anyhow!("Muse key snapshot authentication failed"))?;
                    collection.hash = result.state;
                    collection.macs = result
                        .mutation_macs
                        .into_iter()
                        .map(|m| (m.index_mac, m.value_mac))
                        .collect();
                    // A replacement snapshot can revoke a key by omitting its
                    // record. Do not retain roots from an older snapshot.
                    if next.root_collection.as_deref() == Some(&name) {
                        next.roots.clear();
                        next.active = None;
                    }
                    for (record, mutation) in snapshot.records.iter().zip(result.mutations.iter()) {
                        apply_root_record(&mut next, record, mutation, &keys)?;
                        if mutation.index == ["wasa_root_secret", MUSE_JID] {
                            next.root_collection = Some(name.clone());
                        }
                    }
                }
                for patch in &list.patches {
                    ensure!(
                        patch.key_id.id.is_some()
                            && patch.mutations.iter().all(|m| m.record.is_set()),
                        "incomplete Muse key patch"
                    );
                    let result = appstate::process_patch(
                        patch,
                        &mut collection.hash,
                        get_keys,
                        |id| Ok(collection.macs.get(id).cloned()),
                        true,
                        &name,
                    )
                    .map_err(|_| anyhow!("Muse key patch authentication failed"))?;
                    ensure!(
                        !result.state.mac_mismatch_fatal,
                        "Muse key sync needs a fresh snapshot"
                    );
                    collection.hash = result.state;
                    for id in result.removed_index_macs {
                        collection.macs.remove(&id);
                    }
                    for mac in result.added_macs {
                        collection.macs.insert(mac.index_mac, mac.value_mac);
                    }
                    for (wire, mutation) in patch.mutations.iter().zip(result.mutations.iter()) {
                        let record = wire.record.as_option().context("missing Muse key record")?;
                        apply_root_record(&mut next, record, mutation, &keys)?;
                        if mutation.index == ["wasa_root_secret", MUSE_JID] {
                            next.root_collection = Some(name.clone());
                        }
                    }
                }
                next.collections.insert(name.clone(), collection);
                if list.has_more_patches {
                    more.push(name);
                }
            }
            if more.is_empty() {
                let ready = next.active.is_some();
                *guard = next;
                if crate::protocol_diagnostics::enabled() {
                    eprintln!("medousa_whatsapp Muse key sync complete; ready={ready}");
                }
                return Ok(());
            }
            pending = more;
        }
        Err(anyhow!("Muse key sync exceeded page limit"))
    }

    pub async fn send_text(&self, client: &Arc<Client>, text: String) -> Result<()> {
        let _sending = self.send_lock.lock().await;
        self.refresh().await?;
        let (target_id, root) = {
            let state = self.state.lock().await;
            let id = state
                .active
                .as_ref()
                .context("Muse pairing secret is unavailable; reconnect Muse in WhatsApp")?;
            (
                id.clone(),
                *state
                    .roots
                    .get(id)
                    .context("Muse pairing secret is unavailable")?,
            )
        };
        let bot: Jid = MUSE_JID.parse()?;
        let lid = client
            .lid()
            .context("WhatsApp LID is not ready")?
            .to_non_ad();
        let own = client.pn().context("WhatsApp identity is not ready")?;
        let id = client.generate_message_id();
        let message = wa::Message {
            conversation: Some(text),
            ..Default::default()
        };
        let wrapped = wrap_message(&message, &root, &target_id, &id, &lid.to_string())?;
        let own_devices = client
            .signal()
            .get_user_devices(&[own.to_non_ad()])
            .await
            .map_err(|_| anyhow!("Muse companion device lookup failed"))?
            .into_iter()
            .filter(|jid| jid.device != own.device)
            .collect::<Vec<_>>();
        let mut devices = own_devices.clone();
        devices.push(bot.clone());
        client
            .signal()
            .assert_sessions(&devices)
            .await
            .map_err(|_| anyhow!("Muse Signal session setup failed"))?;
        let companion_message = wa::Message {
            device_sent_message: MessageField::some(wa::message::DeviceSentMessage {
                destination_jid: Some(MUSE_JID.to_string()),
                message: MessageField::some(message),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut participants = Vec::new();
        let mut prekey = false;
        // Fail the entire send if any encryption fails; never send a successful
        // self-echo while silently dropping the bot's copy.
        for device in devices {
            let plain = MessageUtils::encode_and_pad(if is_muse(&device) {
                &wrapped
            } else {
                &companion_message
            });
            let (kind, ciphertext) = client
                .signal()
                .encrypt_message(&device, &plain)
                .await
                .map_err(|_| anyhow!("Muse Signal encryption failed"))?;
            let kind = match kind {
                wacore::message_processing::EncType::PreKeyMessage => {
                    prekey = true;
                    "pkmsg"
                }
                wacore::message_processing::EncType::Message => "msg",
                _ => return Err(anyhow!("unexpected Muse Signal envelope")),
            };
            participants.push(
                NodeBuilder::new("to")
                    .attr("jid", &device)
                    .children(vec![
                        NodeBuilder::new("enc")
                            .attr("v", "2")
                            .attr("type", kind)
                            .bytes(ciphertext)
                            .build(),
                    ])
                    .build(),
            );
        }
        let mut children = vec![
            NodeBuilder::new("participants")
                .children(participants)
                .build(),
        ];
        let snapshot = client.persistence_manager().get_device_snapshot();
        if let Some(identity) =
            wacore::send::needs_device_identity(prekey, snapshot.account.as_deref())?
        {
            children.push(NodeBuilder::new("device-identity").bytes(identity).build());
        }
        children.push(
            NodeBuilder::new("bot")
                .attr("type", "prompt")
                .attr("agent_engagement_type", "direct_chat")
                .build(),
        );
        let waiter = client.wait_for_node(
            NodeFilter::tag("ack")
                .attr("class", "message")
                .attr("id", &id),
        );
        client
            .send_node(
                NodeBuilder::new("message")
                    .attr("to", &bot)
                    .attr("id", &id)
                    .attr("type", "text")
                    .children(children)
                    .build(),
            )
            .await
            .map_err(|_| anyhow!("Muse send failed; delivery is uncertain"))?;
        let ack = tokio::time::timeout(Duration::from_secs(30), waiter)
            .await
            .context("Muse transport acknowledgement timed out; delivery is uncertain")?
            .context("Muse transport acknowledgement interrupted; delivery is uncertain")?;
        ensure!(
            ack.get().get_attr("error").is_none(),
            "WhatsApp rejected the Muse message"
        );
        Ok(())
    }
}

#[async_trait]
impl OriginalMessageResolver for MuseProtocol {
    async fn resolve_msg_secret(&self, chat: &str, sender: &str, msg_id: &str) -> Option<[u8; 32]> {
        if crate::protocol_diagnostics::enabled() {
            eprintln!(
                "medousa_whatsapp protocol: secret-lookup muse={}",
                chat == MUSE_JID
            );
        }
        if chat != MUSE_JID {
            return None;
        }
        let client = self.client.get()?.upgrade()?;
        // Never provide an own-chat root for another sender's identity.
        if ![client.lid(), client.pn()]
            .into_iter()
            .flatten()
            .any(|j| j.to_non_ad().to_string() == sender)
        {
            if crate::protocol_diagnostics::enabled() {
                eprintln!("medousa_whatsapp protocol: secret-lookup sender-mismatch");
            }
            return None;
        }
        if crate::protocol_diagnostics::enabled() {
            eprintln!(
                "medousa_whatsapp protocol: secret-lookup found={}",
                self.state.lock().await.roots.contains_key(msg_id)
            );
        }
        if let Some(secret) = self.state.lock().await.roots.get(msg_id).copied() {
            return Some(secret);
        }
        if self.refresh().await.is_err() {
            return None;
        }
        self.state.lock().await.roots.get(msg_id).copied()
    }
}

// The 0.7 schema omits status (field 4) and discards unknown fields. Decode this
// small projection only AFTER the library has verified snapshot/patch/record MACs.
#[derive(Clone, PartialEq, prost::Message)]
struct SyncData {
    #[prost(message, optional, tag = "2")]
    value: Option<SyncValue>,
}
#[derive(Clone, PartialEq, prost::Message)]
struct SyncValue {
    #[prost(message, optional, tag = "89")]
    roots: Option<RootAction>,
}
#[derive(Clone, PartialEq, prost::Message)]
struct RootAction {
    #[prost(message, repeated, tag = "1")]
    secrets: Vec<RootEntry>,
}
#[derive(Clone, PartialEq, prost::Message)]
struct RootEntry {
    #[prost(string, optional, tag = "1")]
    id: Option<String>,
    #[prost(bytes = "vec", optional, tag = "2")]
    secret: Option<Vec<u8>>,
    #[prost(int64, optional, tag = "3")]
    epoch: Option<i64>,
    #[prost(int32, optional, tag = "4")]
    status: Option<i32>,
}

fn apply_root_record(
    state: &mut SyncState,
    record: &wa::SyncdRecord,
    mutation: &appstate::Mutation,
    keys: &SyncKeys,
) -> Result<()> {
    if mutation.index != ["wasa_root_secret", MUSE_JID] {
        return Ok(());
    }
    let key_id = record
        .key_id
        .id
        .as_deref()
        .context("missing Muse sync key id")?;
    let key = keys.get(key_id).context("missing Muse sync key")?;
    // Keep the raw projection bound to the same authenticated index/operation,
    // even if callers later change how processed mutations are paired with records.
    let (verified, _) = appstate::decode_record(mutation.operation, record, key, key_id, true)
        .map_err(|_| anyhow!("Muse root record authentication failed"))?;
    ensure!(
        verified.index == mutation.index,
        "Muse root record index mismatch"
    );
    if mutation.operation == wa::syncd_mutation::SyncdOperation::Remove {
        state.roots.clear();
        state.active = None;
        return Ok(());
    }
    let blob = record
        .value
        .blob
        .as_deref()
        .context("missing Muse sync value")?;
    ensure!(blob.len() >= 48, "invalid Muse sync value");
    let iv: &[u8; 16] = blob[..16].try_into()?;
    let mut plain = Vec::new();
    aes_256_cbc_decrypt_into(
        &blob[16..blob.len() - 32],
        &key.value_encryption,
        iv,
        &mut plain,
    )
    .map_err(|_| anyhow!("Muse root record decryption failed"))?;
    let action = SyncData::decode(plain.as_slice())
        .context("decode Muse root record")?
        .value
        .and_then(|v| v.roots)
        .context("missing Muse root action")?;
    install_root_action(state, action);
    Ok(())
}

fn install_root_action(state: &mut SyncState, action: RootAction) {
    let mut roots = HashMap::new();
    let mut active: Option<(i64, String)> = None;
    for entry in action.secrets {
        let (Some(id), Some(secret)) = (entry.id, entry.secret) else {
            continue;
        };
        let Ok(secret) = <[u8; 32]>::try_from(secret) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        if entry.status == Some(1)
            && active
                .as_ref()
                .is_none_or(|(epoch, _)| entry.epoch.unwrap_or(0) > *epoch)
        {
            active = Some((entry.epoch.unwrap_or(0), id.clone()));
        }
        roots.insert(id, secret);
    }
    state.roots = roots;
    state.active = active.map(|(_, id)| id);
}

fn wrap_message(
    message: &wa::Message,
    root: &[u8; 32],
    target_id: &str,
    stanza_id: &str,
    own_lid: &str,
) -> Result<wa::Message> {
    let mut iv = [0u8; 12];
    rand::rng().fill(&mut iv);
    wrap_message_with_iv(message, root, target_id, stanza_id, own_lid, &iv)
}

fn wrap_message_with_iv(
    message: &wa::Message,
    root: &[u8; 32],
    target_id: &str,
    stanza_id: &str,
    own_lid: &str,
    iv: &[u8; 12],
) -> Result<wa::Message> {
    let mut base = [0u8; 32];
    wacore::crypto::hkdf_sha256_into(root, None, b"Bot Message", &mut base)?;
    let mut key = [0u8; 32];
    wacore::crypto::hkdf_sha256_into(
        &base,
        None,
        format!("{stanza_id}{own_lid}{MUSE_JID}").as_bytes(),
        &mut key,
    )?;
    let plaintext = waproto::codec::message_to_vec(message);
    let mut encrypted = Vec::new();
    // Outbound AAD binds the USER; inbound msmsg binds the BOT. These differ.
    aes_256_gcm_encrypt(
        &key,
        iv,
        format!("{stanza_id}\0{own_lid}").as_bytes(),
        &plaintext,
        &mut encrypted,
    )?;
    Ok(wa::Message {
        secret_encrypted_message: MessageField::some(wa::message::SecretEncryptedMessage {
            target_message_key: MessageField::some(wa::MessageKey {
                remote_jid: Some(MUSE_JID.to_string()),
                from_me: Some(true),
                id: Some(target_id.to_string()),
                ..Default::default()
            }),
            enc_payload: Some(encrypted),
            enc_iv: Some(iv.to_vec()),
            // Web omits secretEncType for WASA (this is not an encrypted edit).
            ..Default::default()
        }),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbound_matches_independent_node_crypto_vector() {
        // Node crypto HKDF-SHA256 + AES-256-GCM, synthetic identities and keys.
        let msg = wa::Message {
            conversation: Some("ping".into()),
            ..Default::default()
        };
        let envelope = wrap_message_with_iv(
            &msg,
            &[0x42; 32],
            "ROOT-ID",
            "TEST-STANZA",
            "100000000000001@lid",
            &[0x24; 12],
        )
        .unwrap();
        let encrypted = envelope.secret_encrypted_message.as_option().unwrap();
        let expected = [
            0x12, 0x9d, 0x34, 0xdb, 0xda, 0x4c, 0x24, 0x29, 0x21, 0x80, 0x0c, 0x62, 0x6e, 0x42,
            0x41, 0x87, 0x24, 0xad, 0x1d, 0xb6, 0xd0, 0xdd,
        ];
        assert_eq!(encrypted.enc_payload.as_deref(), Some(expected.as_slice()));
        assert_eq!(encrypted.target_message_key.id.as_deref(), Some("ROOT-ID"));
        assert_eq!(encrypted.target_message_key.from_me, Some(true));
        assert_eq!(
            encrypted.target_message_key.remote_jid.as_deref(),
            Some(MUSE_JID)
        );
        assert!(encrypted.secret_enc_type.is_none());
        // Using the inbound helper would bind the BOT in AAD and fail here.
        assert!(
            wacore::bot_message::decrypt_bot_message(
                &[0x42; 32],
                &[0x24; 12],
                &expected,
                &wacore::bot_message::BotMessageContext {
                    msg_id: "TEST-STANZA",
                    target_sender_user_jid: "100000000000001@lid",
                    bot_user_jid: MUSE_JID
                }
            )
            .is_err()
        );
    }

    fn root(id: &str, epoch: i64, status: Option<i32>) -> RootEntry {
        RootEntry {
            id: Some(id.into()),
            epoch: Some(epoch),
            status,
            secret: Some(vec![0x42; 32]),
        }
    }

    #[test]
    fn only_latest_explicitly_active_root_can_send() {
        let mut state = SyncState::default();
        install_root_action(
            &mut state,
            RootAction {
                secrets: vec![
                    root("old", 1, Some(1)),
                    root("current", 3, Some(1)),
                    root("inactive", 9, Some(0)),
                    root("unknown", 10, Some(8)),
                    root("absent", 11, None),
                ],
            },
        );
        assert_eq!(state.active.as_deref(), Some("current"));
        assert_eq!(state.roots.len(), 5); // Old roots remain usable for inbound replies.
        install_root_action(
            &mut state,
            RootAction {
                secrets: vec![root("current", 3, Some(0))],
            },
        );
        assert!(state.active.is_none());
    }

    #[test]
    fn missing_or_malformed_roots_fail_closed() {
        let mut state = SyncState::default();
        let mut bad = root("bad", 1, Some(1));
        bad.secret = Some(vec![0; 31]);
        install_root_action(
            &mut state,
            RootAction {
                secrets: vec![bad, root("", 2, Some(1))],
            },
        );
        assert!(state.active.is_none());
        assert!(state.roots.is_empty());
    }

    #[test]
    fn unified_only_reply_reaches_text_extraction() {
        let data = serde_json::json!({"sections": [
            {"view_model": {"__typename": "GenAISingleLayoutViewModel", "primitive": {
                "__typename": "GenAIMarkdownTextUXPrimitive", "text": "{{emphasis}}ROUNDTRIP-OK{{/emphasis}}"
            }}},
            {"view_model": {"__typename": "GenAIGridLayoutViewModel", "primitives": [
                {"__typename": "GenAICodeUXPrimitive", "code_blocks": [{"content": "let x = 1;"}]},
                {"__typename": "GenATableUXPrimitive", "rows": [{"cells": ["a", "b"]}]},
                {"__typename": "GenAILatexUXPrimitive", "item": {"latex_expression": "x^2"}},
                {"__typename": "Unknown", "text": "PRIVATE-METADATA"}
            ]}},
            {"view_model": {"__typename": "UnknownLayout", "primitive": {"text": "PRIVATE-METADATA"}}}
        ], "debug": {"text": "PRIVATE-METADATA"}});
        let message = wa::Message {
            rich_response_message: MessageField::some(wa::AIRichResponseMessage {
                unified_response: MessageField::some(wa::AIRichResponseUnifiedResponse {
                    data: Some(serde_json::to_vec(&data).unwrap()),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            message_text(&message).as_deref(),
            Some("ROUNDTRIP-OK\nlet x = 1;\na | b\nx^2")
        );
        assert_eq!(
            strip_rich_tags("literal {{unclosed}} and {{x}}OK{{/x}}"),
            "literal {{unclosed}} and OK"
        );
        assert!(unified_response_text(b"not json").is_none());
        assert!(unified_response_text(br#"{"text":"not a displayed primitive"}"#).is_none());
    }

    #[test]
    fn streaming_protocol_edit_extracts_the_complete_rich_reply() {
        let text = format!("{}END-OF-REPLY", "A complete paragraph.\n\n".repeat(240));
        let data = serde_json::json!({"sections": [{"view_model": {
            "__typename": "GenAISingleLayoutViewModel", "primitive": {
                "__typename": "GenAIMarkdownTextUXPrimitive", "text": text
            }
        }}]});
        let message = wa::Message {
            edited_message: MessageField::some(wa::message::FutureProofMessage {
                message: MessageField::some(wa::Message {
                    protocol_message: MessageField::some(wa::message::ProtocolMessage {
                        r#type: Some(wa::message::protocol_message::Type::MESSAGE_EDIT),
                        edited_message: MessageField::some(wa::Message {
                            rich_response_message: MessageField::some(wa::AIRichResponseMessage {
                                unified_response: MessageField::some(
                                    wa::AIRichResponseUnifiedResponse {
                                        data: Some(serde_json::to_vec(&data).unwrap()),
                                    },
                                ),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            }),
            ..Default::default()
        };
        assert_eq!(message_text(&message).as_deref(), Some(text.as_str()));
    }

    #[test]
    fn streaming_previews_wait_for_explicit_last_without_a_timeout() {
        let preview = wa::Message {
            conversation: Some("A partial".into()),
            ..Default::default()
        };
        let final_message = wa::Message {
            protocol_message: MessageField::some(wa::message::ProtocolMessage {
                r#type: Some(wa::message::protocol_message::Type::MESSAGE_EDIT),
                edited_message: MessageField::some(wa::Message {
                    conversation: Some("A complete reply, including its final sentence.".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let bot = |edit_type| MsgBotInfo {
            edit_type: Some(edit_type),
            edit_target_id: Some("synthetic-original-reply".into()),
            edit_sender_timestamp_ms: None,
        };
        assert!(completed_message_text(&preview, Some(&bot(BotEditType::First))).is_none());
        assert!(completed_message_text(&final_message, Some(&bot(BotEditType::Inner))).is_none());
        // No first/inner state is required: a final arriving after reconnect
        // still carries a complete replacement and must be displayed.
        assert_eq!(
            completed_message_text(&final_message, Some(&bot(BotEditType::Last))).as_deref(),
            Some("A complete reply, including its final sentence.")
        );
        assert_eq!(
            completed_message_text(&preview, None).as_deref(),
            Some("A partial")
        );
    }

    #[test]
    fn unrelated_protocol_messages_are_not_displayed_as_reply_text() {
        let message = wa::Message {
            protocol_message: MessageField::some(wa::message::ProtocolMessage {
                r#type: Some(wa::message::protocol_message::Type::REVOKE),
                edited_message: MessageField::some(wa::Message {
                    conversation: Some("not a reply".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(message_text(&message).is_none());
    }

    #[test]
    fn legacy_rich_reply_survives_invalid_unified_data() {
        let message = wa::Message {
            rich_response_message: MessageField::some(wa::AIRichResponseMessage {
                unified_response: MessageField::some(wa::AIRichResponseUnifiedResponse {
                    data: Some(vec![0xff]),
                }),
                submessages: vec![
                    wa::AIRichResponseSubMessage {
                        message_text: Some("First".into()),
                        ..Default::default()
                    },
                    wa::AIRichResponseSubMessage {
                        message_text: Some(" ".into()),
                        ..Default::default()
                    },
                    wa::AIRichResponseSubMessage {
                        message_text: Some("Second".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(message_text(&message).as_deref(), Some("First\n\nSecond"));
    }

    fn encrypted_root_record(
        operation: wa::syncd_mutation::SyncdOperation,
    ) -> (wa::SyncdRecord, appstate::Mutation, SyncKeys) {
        use wacore::appstate::hash::{generate_content_mac, generate_index_mac};
        use wacore::libsignal::crypto::aes_256_cbc_encrypt_into;
        // Include the new status field which waproto 0.7 silently discards.
        let roots = SyncData {
            value: Some(SyncValue {
                roots: Some(RootAction {
                    secrets: vec![root("synthetic-root", 7, Some(1))],
                }),
            }),
        }
        .encode_to_vec();
        let index = serde_json::to_vec(&["wasa_root_secret", MUSE_JID]).unwrap();
        let mut plain = waproto::codec::sync_action_data_to_vec(&wa::SyncActionData {
            index: Some(index.clone()),
            ..Default::default()
        });
        plain.extend_from_slice(&roots);
        let key_id = b"synthetic-sync-key".to_vec();
        let key = Arc::new(appstate::expand_app_state_keys(&[0x17; 32]));
        let mut cipher = Vec::new();
        aes_256_cbc_encrypt_into(&plain, &key.value_encryption, &[0x28; 16], &mut cipher).unwrap();
        let mut blob = vec![0x28; 16];
        blob.extend_from_slice(&cipher);
        blob.extend_from_slice(&generate_content_mac(
            operation,
            &blob,
            &key_id,
            &key.value_mac,
        ));
        let record = wa::SyncdRecord {
            key_id: MessageField::some(wa::KeyId {
                id: Some(key_id.clone()),
            }),
            index: MessageField::some(wa::SyncdIndex {
                blob: Some(generate_index_mac(&index, &key.index)),
            }),
            value: MessageField::some(wa::SyncdValue { blob: Some(blob) }),
        };
        let (mutation, _) =
            appstate::decode_record(operation, &record, &key, &key_id, true).unwrap();
        (record, mutation, HashMap::from([(key_id, key)]))
    }

    #[test]
    fn authenticated_root_preserves_new_status_and_remove_revokes_it() {
        let mut state = SyncState::default();
        let (record, mutation, keys) =
            encrypted_root_record(wa::syncd_mutation::SyncdOperation::Set);
        apply_root_record(&mut state, &record, &mutation, &keys).unwrap();
        assert_eq!(state.active.as_deref(), Some("synthetic-root"));
        assert_eq!(state.roots.get("synthetic-root"), Some(&[0x42; 32]));
        let (record, mutation, keys) =
            encrypted_root_record(wa::syncd_mutation::SyncdOperation::Remove);
        apply_root_record(&mut state, &record, &mutation, &keys).unwrap();
        assert!(state.active.is_none());
        assert!(state.roots.is_empty());
    }

    #[test]
    fn tampered_root_record_cannot_replace_a_working_secret() {
        let mut state = SyncState::default();
        install_root_action(
            &mut state,
            RootAction {
                secrets: vec![root("working", 1, Some(1))],
            },
        );
        let (mut record, mutation, keys) =
            encrypted_root_record(wa::syncd_mutation::SyncdOperation::Set);
        let mut blob = record.value.blob.clone().unwrap();
        *blob.last_mut().unwrap() ^= 1;
        record.value = MessageField::some(wa::SyncdValue { blob: Some(blob) });
        assert!(apply_root_record(&mut state, &record, &mutation, &keys).is_err());
        assert_eq!(state.active.as_deref(), Some("working"));
        assert!(!state.roots.contains_key("synthetic-root"));
    }

    #[test]
    fn service_identity_does_not_match_other_bots() {
        assert!(is_muse(&MUSE_JID.parse().unwrap()));
        assert!(!is_muse(&"867051314767696@bot".parse().unwrap()));
    }
}
