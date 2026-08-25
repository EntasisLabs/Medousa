#[cfg(feature = "async")]
use medousa_types::{
    InteractiveTurnRequest, InteractiveTurnResponse, InteractiveTurnStreamEvent,
    TurnStreamEnvelopeV2, TurnStreamEnvelopeV3,
};

#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::Stream;
#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::StreamExt;

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::op_path;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(all(feature = "async", feature = "sse"))]
use crate::reconnecting_stream::{
    ReconnectingInteractiveStream, ReconnectingInteractiveStreamV2, ReconnectingInteractiveStreamV3,
};
#[cfg(all(feature = "async", feature = "sse"))]
use crate::streaming::{SseLineStream, decode_sse_json};

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
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                ops::INTERACTIVE_TURN_POST.path,
                body,
            )
            .await?;
        decode(value).await
    }

    pub async fn cancel(&self, session_id: &str) -> Result<serde_json::Value, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_ACTIVE_TURN_POST,
            &[("session_id", session_id)],
        )?;
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

    /// Open the recommended typed v2 stream with spine-backed reconnect.
    #[cfg(feature = "sse")]
    pub fn stream_reconnecting_v2(
        &self,
        stream_url: impl Into<String>,
    ) -> ReconnectingInteractiveStreamV2<'_> {
        ReconnectingInteractiveStreamV2::new_v2(self.client, stream_url)
    }

    /// Open the typed v2 stream with a custom reconnect policy.
    #[cfg(feature = "sse")]
    pub fn stream_reconnecting_v2_with_policy(
        &self,
        stream_url: impl Into<String>,
        policy: crate::ReconnectPolicy,
    ) -> ReconnectingInteractiveStreamV2<'_> {
        ReconnectingInteractiveStreamV2::with_policy_v2(self.client, stream_url, policy)
    }

    /// Open the native chronological v3 stream with spine-backed reconnect.
    #[cfg(feature = "sse")]
    pub fn stream_reconnecting_v3(
        &self,
        stream_url: impl Into<String>,
    ) -> ReconnectingInteractiveStreamV3<'_> {
        ReconnectingInteractiveStreamV3::new_v3(self.client, stream_url)
    }

    /// Open the native chronological v3 stream with a custom reconnect policy.
    #[cfg(feature = "sse")]
    pub fn stream_reconnecting_v3_with_policy(
        &self,
        stream_url: impl Into<String>,
        policy: crate::ReconnectPolicy,
    ) -> ReconnectingInteractiveStreamV3<'_> {
        ReconnectingInteractiveStreamV3::with_policy_v3(self.client, stream_url, policy)
    }

    #[cfg(feature = "sse")]
    pub async fn stream_turn_reconnecting(
        &self,
        request: &InteractiveTurnRequest,
    ) -> Result<ReconnectingInteractiveStream<'_>, crate::SdkError> {
        let response = self.start_turn(request).await?;
        Ok(self.stream_reconnecting(response.stream_url))
    }

    /// Start a turn and follow its typed v2 stream through bounded reconnects.
    #[cfg(feature = "sse")]
    pub async fn stream_turn_reconnecting_v2(
        &self,
        request: &InteractiveTurnRequest,
    ) -> Result<ReconnectingInteractiveStreamV2<'_>, crate::SdkError> {
        let response = self.start_turn(request).await?;
        Ok(self.stream_reconnecting_v2(response.stream_url))
    }

    /// Start a turn and follow its native chronological v3 facts.
    #[cfg(feature = "sse")]
    pub async fn stream_turn_reconnecting_v3(
        &self,
        request: &InteractiveTurnRequest,
    ) -> Result<ReconnectingInteractiveStreamV3<'_>, crate::SdkError> {
        let response = self.start_turn(request).await?;
        Ok(self.stream_reconnecting_v3(response.stream_url))
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

    /// Open the typed v2 turn stream without reconnecting.
    #[cfg(feature = "sse")]
    pub fn stream_v2(
        &self,
        stream_url: impl Into<String>,
    ) -> impl Stream<Item = Result<TurnStreamEnvelopeV2, crate::SdkError>> + '_ {
        let byte_stream = self.client.transport().stream_sse_with_accept(
            self.client.base_url(),
            stream_url.into(),
            medousa_types::turn_stream::TURN_STREAM_V2_MEDIA_TYPE,
        );
        SseLineStream::new(byte_stream).map(|line| line.and_then(|data| decode_sse_json(&data)))
    }

    /// Open the native chronological v3 turn stream without reconnecting.
    #[cfg(feature = "sse")]
    pub fn stream_v3(
        &self,
        stream_url: impl Into<String>,
    ) -> impl Stream<Item = Result<TurnStreamEnvelopeV3, crate::SdkError>> + '_ {
        let byte_stream = self.client.transport().stream_sse_with_accept(
            self.client.base_url(),
            stream_url.into(),
            medousa_types::turn_stream::TURN_STREAM_V3_MEDIA_TYPE,
        );
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
