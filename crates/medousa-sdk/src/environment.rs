#[cfg(feature = "async")]
use medousa_types::{
    EnvironmentPendingResponse, EnvironmentProposeResponse, EnvironmentSpecPutRequest,
    EnvironmentSpecResponse, EnvironmentStatusResponse, EnvironmentStreamEvent,
    EnvironmentValidateRequest, EnvironmentValidateResponse,
};

#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::Stream;
#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::StreamExt;

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::path_with_query;

#[cfg(all(feature = "async", feature = "sse"))]
use crate::streaming::{SseLineStream, decode_sse_json};

#[cfg(feature = "async")]
pub struct EnvironmentApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
fn profile_query(profile_id: Option<&str>) -> Vec<(&'static str, String)> {
    profile_id
        .map(|value| vec![("profile_id", value.to_string())])
        .unwrap_or_default()
}

#[cfg(feature = "async")]
fn stream_query(profile_id: Option<&str>, since_revision: Option<u64>) -> Vec<(&'static str, String)> {
    let mut params = profile_query(profile_id);
    if let Some(since) = since_revision {
        params.push(("since_revision", since.to_string()));
    }
    params
}

#[cfg(feature = "async")]
impl EnvironmentApi<'_> {
    pub async fn get_spec(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentSpecResponse, crate::SdkError> {
        let path = path_with_query("/v1/environment/spec", &profile_query(profile_id));
        self.client.http().get(&path).await
    }

    pub async fn put_spec(
        &self,
        request: &EnvironmentSpecPutRequest,
    ) -> Result<EnvironmentSpecResponse, crate::SdkError> {
        self.client
            .http()
            .put("/v1/environment/spec", request)
            .await
    }

    pub async fn get_status(
        &self,
        profile_id: Option<&str>,
        surface_id: Option<&str>,
        include_runtime: Option<bool>,
    ) -> Result<EnvironmentStatusResponse, crate::SdkError> {
        let mut params = profile_query(profile_id);
        if let Some(surface) = surface_id {
            params.push(("surface_id", surface.to_string()));
        }
        if let Some(include) = include_runtime {
            params.push(("include_runtime", include.to_string()));
        }
        let path = path_with_query("/v1/environment/status", &params);
        self.client.http().get(&path).await
    }

    pub async fn validate_spec(
        &self,
        request: &EnvironmentValidateRequest,
    ) -> Result<EnvironmentValidateResponse, crate::SdkError> {
        self.client
            .http()
            .post("/v1/environment/spec/validate", request)
            .await
    }

    pub async fn propose_spec(
        &self,
        request: &EnvironmentSpecPutRequest,
    ) -> Result<EnvironmentProposeResponse, crate::SdkError> {
        self.client
            .http()
            .post("/v1/environment/spec/propose", request)
            .await
    }

    pub async fn get_pending(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentPendingResponse, crate::SdkError> {
        let path = path_with_query("/v1/environment/spec/pending", &profile_query(profile_id));
        self.client.http().get(&path).await
    }

    pub async fn dismiss_pending(&self, profile_id: Option<&str>) -> Result<(), crate::SdkError> {
        let path = path_with_query("/v1/environment/spec/pending", &profile_query(profile_id));
        self.client.http().delete::<serde_json::Value>(&path).await?;
        Ok(())
    }

    pub async fn apply_pending(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentSpecResponse, crate::SdkError> {
        let path = path_with_query(
            "/v1/environment/spec/pending/apply",
            &profile_query(profile_id),
        );
        self.client.http().post_empty(&path).await
    }

    #[cfg(feature = "sse")]
    pub fn stream_spec(
        &self,
        profile_id: Option<&str>,
        since_revision: Option<u64>,
    ) -> impl Stream<Item = Result<EnvironmentStreamEvent, crate::SdkError>> + '_ {
        let path = path_with_query(
            "/v1/environment/spec/stream",
            &stream_query(profile_id, since_revision),
        );
        let byte_stream = self
            .client
            .transport()
            .stream_sse(self.client.base_url(), path);
        SseLineStream::new(byte_stream).map(|line| line.and_then(|data| decode_sse_json(&data)))
    }
}
