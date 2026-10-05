//! Existing content identities: artifact payload revisions, environment components,
//! and retained feed streams remain separate native contracts.
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkContentResolveInput {
    pub target: ContentResolveTarget,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContentResolveTarget {
    Artifact {
        session_id: String,
        artifact_id: String,
    },
    Component {
        component_id: String,
    },
    Feed {
        feed_id: String,
    },
    Reference {
        reference: ResourceRef,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ContentLocator {
    Artifact {
        session_id: String,
        artifact_id: String,
    },
    Component {
        component_id: String,
    },
    Feed {
        feed_id: String,
    },
}
impl ContentLocator {
    fn kind(&self) -> ResourceKind {
        match self {
            Self::Artifact { .. } => ResourceKind::Artifact,
            Self::Component { .. } => ResourceKind::Component,
            Self::Feed { .. } => ResourceKind::Feed,
        }
    }
    fn validate(&self) -> Result<()> {
        let id = match self {
            Self::Artifact {
                session_id,
                artifact_id,
            } => {
                medousa_types::SessionId::parse(session_id)?;
                artifact_id
            }
            Self::Component { component_id } => component_id,
            Self::Feed { feed_id } => {
                medousa_types::authority_id::FeedId::parse(feed_id)?;
                feed_id
            }
        };
        if id.is_empty() || id.len() > 256 || id.trim() != id || id.chars().any(char::is_control) {
            bail!(
                "native content identity must be 1–256 bytes without whitespace padding or controls"
            );
        }
        Ok(())
    }
    fn reference(&self, domain: &UserDomainRef) -> Result<ResourceRef> {
        self.validate()?;
        Ok(ResourceRef {
            authority_id: domain.authority_id.clone(),
            kind: self.kind(),
            // Include the full source session and owner: legacy artifact ids only
            // carry a short session prefix and cannot alone identify a payload.
            id: format!(
                "content-v1:{:x}",
                Sha256::digest(serde_json::to_vec(&(&domain.user_id, self))?)
            ),
        })
    }
}

fn publish(
    graph: &WorkGraphStore,
    domain: &UserDomainRef,
    locator: &ContentLocator,
    available: bool,
    revision: String,
    bindings: serde_json::Value,
    coverage: &str,
) -> Result<serde_json::Value> {
    let mut value = native_graph::publish(
        graph,
        domain,
        locator.reference(domain)?,
        serde_json::to_string(locator)?,
        revision,
        if available {
            ResourceResolution::Available
        } else {
            ResourceResolution::Unavailable
        },
        "native-content",
    )?;
    value["coverage"] = coverage.into();
    value["bindings"] = bindings;
    value["file_effects_replayed"] = false.into();
    bounded_response(value)
}

fn observe_artifact(
    graph: &WorkGraphStore,
    domain: &UserDomainRef,
    locator: &ContentLocator,
    session_id: &str,
    artifact_id: &str,
) -> Result<serde_json::Value> {
    let (_, _lease) = crate::session_deletion::acquire_mutation_for_str(session_id)
        .map_err(anyhow::Error::msg)?;
    if !crate::session_catalog::session_visible_to_profile(session_id, &domain.user_id) {
        bail!("artifact source session is not visible to this owner");
    }
    crate::artifact_store::observe_exact(session_id, artifact_id, |record, available| {
        let revision = format!(
            "artifact-v1:{:x}",
            Sha256::digest(serde_json::to_vec(&record)?)
        );
        let lineage = serde_json::json!({"source_session": ResourceRef { authority_id: domain.authority_id.clone(), kind: ResourceKind::Session, id: session_id.to_string() }});
        publish(
            graph,
            domain,
            locator,
            available,
            revision,
            lineage,
            "artifact_index_and_payload_presence",
        )
    })
}

impl WorkUnitHost {
    pub async fn resolve_content(
        &self,
        turn: &TurnExecutionContext,
        input: WorkContentResolveInput,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, true)?;
        self.resolve_content_in(
            &domain,
            input,
            crate::environment_store::environment_hub(),
            crate::feed_store::feed_store(),
        )
        .await
    }

    async fn resolve_content_in(
        &self,
        domain: &UserDomainRef,
        input: WorkContentResolveInput,
        environment: &crate::environment_store::EnvironmentHub,
        feeds: &crate::feed_store::FeedStore,
    ) -> Result<serde_json::Value> {
        let locator = match input.target {
            ContentResolveTarget::Artifact {
                session_id,
                artifact_id,
            } => ContentLocator::Artifact {
                session_id,
                artifact_id,
            },
            ContentResolveTarget::Component { component_id } => {
                ContentLocator::Component { component_id }
            }
            ContentResolveTarget::Feed { feed_id } => ContentLocator::Feed { feed_id },
            ContentResolveTarget::Reference { reference } => {
                if reference.authority_id != domain.authority_id
                    || !matches!(
                        reference.kind,
                        ResourceKind::Artifact | ResourceKind::Component | ResourceKind::Feed
                    )
                {
                    bail!("reference is not native content on this authority");
                }
                let query_domain = domain.clone();
                let query_reference = reference.clone();
                let page = self
                    .with_store(move |store| {
                        Ok(store.query(
                            &query_domain,
                            WorkGraphQuery {
                                anchor: Some(query_reference),
                                ..Default::default()
                            },
                        )?)
                    })
                    .await?;
                let Some(WorkGraphItem::Resource(record)) = page.items.first() else {
                    bail!("content reference has not been observed in this domain");
                };
                if record.provenance.source != RecordSource::SystemEvent
                    || record.provenance.actor_id != "native-content"
                {
                    bail!("content reference has no native custody");
                }
                let locator: ContentLocator = serde_json::from_str(
                    record
                        .locator
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("missing native content locator"))?,
                )?;
                if locator.reference(domain)? != reference {
                    bail!("content reference does not match its native locator");
                }
                locator
            }
        };
        locator.validate()?;
        let graph = self.store.clone();
        let domain = domain.clone();
        let owner = domain.user_id.clone();
        match locator.clone() {
            ContentLocator::Artifact { session_id, artifact_id } => {
                self.execution
                    .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                        Ok(observe_artifact(&graph, &domain, &locator, &session_id, &artifact_id))
                    })
                    .await?
            }
            ContentLocator::Component { component_id } => {
                environment
                    .observe_component(&owner, &component_id, &self.execution, |component| async move {
                        if component.as_ref().is_some_and(|item| item.feeds.len() > 128) {
                            bail!("component feed bindings exceed the 128-reference observation limit");
                        }
                        let revision = format!(
                            "component-v1:{:x}",
                            Sha256::digest(serde_json::to_vec(&component)?)
                        );
                        let feed_refs = component.as_ref().map(|item| {
                            item.feeds.iter().map(|feed_id| {
                                ContentLocator::Feed { feed_id: feed_id.clone() }.reference(&domain)
                            }).collect::<Result<Vec<_>>>()
                        }).transpose()?.unwrap_or_default();
                        let artifact_id = component.as_ref()
                            .and_then(|item| item.config.get("artifactId").or_else(|| item.config.get("artifact_id")))
                            .and_then(serde_json::Value::as_str);
                        if artifact_id.is_some_and(|id| id.len() > 256 || id.chars().any(char::is_control)) {
                            bail!("invalid native component artifact binding");
                        }
                        let artifact_binding = artifact_id.map(|id| serde_json::json!({
                            "artifact_id": id,
                            "resolution": "unresolved",
                            "requires": "exact source session and artifact identity"
                        }));
                        let available = component.is_some();
                        self.execution
                            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                                Ok(publish(
                                    &graph, &domain, &locator, available, revision,
                                    serde_json::json!({"feeds": feed_refs, "artifact": artifact_binding}),
                                    "environment_component_configuration",
                                ))
                            })
                            .await?
                    })
                    .await
            }
            ContentLocator::Feed { feed_id } => {
                feeds
                    .observe_metadata(&owner, &feed_id, |available, revision| async move {
                        self.execution
                            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                                Ok(publish(
                                    &graph, &domain, &locator, available, revision,
                                    serde_json::json!({}), "daemon_retained_feed_stream",
                                ))
                            })
                            .await?
                    })
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests;
