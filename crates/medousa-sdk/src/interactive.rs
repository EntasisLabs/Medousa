#[cfg(feature = "async")]
use medousa_types::{InteractiveTurnRequest, InteractiveTurnResponse, InteractiveTurnStreamEvent};

#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::Stream;
#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::StreamExt;

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(all(feature = "async", feature = "sse"))]
use crate::streaming::{SseLineStream, decode_sse_json};
#[cfg(all(feature = "async", feature = "sse"))]
use crate::reconnecting_stream::ReconnectingInteractiveStream;

#[cfg(feature = "async")]
pub struct InteractiveApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl InteractiveApi<'_> {
    pub async fn start_turn(
        &self,
        request: &InteractiveTurnRequest,
    ) -> Result<InteractiveTurnResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/interactive/turn", body)
            .await?;
        decode(value).await
    }

    pub async fn cancel(&self, session_id: &str) -> Result<serde_json::Value, crate::SdkError> {
        let path = format!("/v1/sessions/{session_id}/active-turn");
        self.client
            .transport()
            .post_empty_json(self.client.base_url(), &path)
            .await
    }

    #[cfg(feature = "sse")]
    pub fn stream_reconnecting(
        &self,
        stream_url: impl Into<String>,
    ) -> ReconnectingInteractiveStream<'_> {
        ReconnectingInteractiveStream::new(self.client, stream_url)
    }

    #[cfg(feature = "sse")]
    pub fn stream_reconnecting_with_policy(
        &self,
        stream_url: impl Into<String>,
        policy: crate::ReconnectPolicy,
    ) -> ReconnectingInteractiveStream<'_> {
        ReconnectingInteractiveStream::with_policy(self.client, stream_url, policy)
    }

    #[cfg(feature = "sse")]
    pub async fn stream_turn_reconnecting(
        &self,
        request: &InteractiveTurnRequest,
    ) -> Result<ReconnectingInteractiveStream<'_>, crate::SdkError> {
        let response = self.start_turn(request).await?;
        Ok(self.stream_reconnecting(response.stream_url))
    }

    #[cfg(feature = "sse")]
    pub fn stream(
        &self,
        stream_url: impl Into<String>,
    ) -> impl Stream<Item = Result<InteractiveTurnStreamEvent, crate::SdkError>> + '_ {
        let byte_stream = self
            .client
            .transport()
            .stream_sse(self.client.base_url(), stream_url.into());
        SseLineStream::new(byte_stream).map(|line| line.and_then(|data| decode_sse_json(&data)))
    }

    #[cfg(feature = "sse")]
    pub async fn stream_turn(
        &self,
        request: &InteractiveTurnRequest,
    ) -> Result<
        impl Stream<Item = Result<InteractiveTurnStreamEvent, crate::SdkError>> + '_,
        crate::SdkError,
    > {
        let response = self.start_turn(request).await?;
        Ok(self.stream(response.stream_url))
    }
}
