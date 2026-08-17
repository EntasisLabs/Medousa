#[cfg(feature = "async")]
use medousa_types::{
    ComponentRuntimeEventsRequest, ComponentRuntimeEventsResponse,
    ComponentRuntimeEventsTailResponse, ComponentRuntimeProbeResult, ComponentStoreDeleteResponse,
    ComponentStoreGetResponse, ComponentStoreListResponse, ComponentStoreSetRequest,
    ComponentStoreSetResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::{op_path, op_path_query};

#[cfg(feature = "async")]
pub struct ComponentsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
fn component_store_query(
    profile_id: Option<&str>,
    key: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut params = Vec::new();
    if let Some(profile) = profile_id {
        params.push(("profile_id", profile.to_string()));
    }
    if let Some(key) = key {
        params.push(("key", key.to_string()));
    }
    params
}

#[cfg(feature = "async")]
fn component_profile_query(profile_id: Option<&str>) -> Vec<(&'static str, String)> {
    profile_id
        .map(|value| vec![("profile_id", value.to_string())])
        .unwrap_or_default()
}

#[cfg(feature = "async")]
impl ComponentsApi<'_> {
    pub async fn store_get(
        &self,
        component_id: &str,
        profile_id: Option<&str>,
        key: Option<&str>,
    ) -> Result<ComponentStoreGetResponse, crate::SdkError> {
        let path = op_path_query(
            &ops::COMPONENTS_BY_COMPONENT_ID_STORE_GET,
            &[("component_id", component_id.trim())],
            &component_store_query(profile_id, key),
        )?;
        self.client.http().get(&path).await
    }

    pub async fn store_set(
        &self,
        component_id: &str,
        key: &str,
        request: &ComponentStoreSetRequest,
    ) -> Result<ComponentStoreSetResponse, crate::SdkError> {
        let path = op_path_query(
            &ops::COMPONENTS_BY_COMPONENT_ID_STORE_PUT,
            &[("component_id", component_id.trim())],
            &component_store_query(None, Some(key)),
        )?;
        self.client.http().put(&path, request).await
    }

    pub async fn store_list_keys(
        &self,
        component_id: &str,
        profile_id: Option<&str>,
    ) -> Result<ComponentStoreListResponse, crate::SdkError> {
        let path = op_path_query(
            &ops::COMPONENTS_BY_COMPONENT_ID_STORE_KEYS_GET,
            &[("component_id", component_id.trim())],
            &component_profile_query(profile_id),
        )?;
        self.client.http().get(&path).await
    }

    pub async fn store_get_key(
        &self,
        component_id: &str,
        key: &str,
        profile_id: Option<&str>,
    ) -> Result<ComponentStoreGetResponse, crate::SdkError> {
        let path = op_path_query(
            &ops::COMPONENTS_BY_COMPONENT_ID_STORE_BY_KEY_GET,
            &[("component_id", component_id.trim()), ("key", key.trim())],
            &component_profile_query(profile_id),
        )?;
        self.client.http().get(&path).await
    }

    pub async fn store_put_key(
        &self,
        component_id: &str,
        key: &str,
        request: &ComponentStoreSetRequest,
    ) -> Result<ComponentStoreSetResponse, crate::SdkError> {
        let path = op_path(
            &ops::COMPONENTS_BY_COMPONENT_ID_STORE_BY_KEY_PUT,
            &[("component_id", component_id.trim()), ("key", key.trim())],
        )?;
        self.client.http().put(&path, request).await
    }

    pub async fn store_delete_key(
        &self,
        component_id: &str,
        key: &str,
        profile_id: Option<&str>,
    ) -> Result<ComponentStoreDeleteResponse, crate::SdkError> {
        let path = op_path_query(
            &ops::COMPONENTS_BY_COMPONENT_ID_STORE_BY_KEY_DELETE,
            &[("component_id", component_id.trim()), ("key", key.trim())],
            &component_profile_query(profile_id),
        )?;
        self.client.http().delete(&path).await
    }

    pub async fn runtime_tail_events(
        &self,
        component_id: &str,
        profile_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<ComponentRuntimeEventsTailResponse, crate::SdkError> {
        let mut params = component_profile_query(profile_id);
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        let path = op_path_query(
            &ops::COMPONENTS_BY_COMPONENT_ID_RUNTIME_EVENTS_GET,
            &[("component_id", component_id.trim())],
            &params,
        )?;
        self.client.http().get(&path).await
    }

    pub async fn runtime_append_events(
        &self,
        component_id: &str,
        request: &ComponentRuntimeEventsRequest,
    ) -> Result<ComponentRuntimeEventsResponse, crate::SdkError> {
        let path = op_path(
            &ops::COMPONENTS_BY_COMPONENT_ID_RUNTIME_EVENTS_POST,
            &[("component_id", component_id.trim())],
        )?;
        self.client.http().post(&path, request).await
    }

    pub async fn runtime_complete_probe(
        &self,
        component_id: &str,
        probe_id: &str,
        request: &ComponentRuntimeProbeResult,
    ) -> Result<serde_json::Value, crate::SdkError> {
        let path = op_path(
            &ops::COMPONENTS_BY_COMPONENT_ID_RUNTIME_PROBE_BY_PROBE_ID_RESULT_POST,
            &[
                ("component_id", component_id.trim()),
                ("probe_id", probe_id.trim()),
            ],
        )?;
        self.client.http().post(&path, request).await
    }
}
