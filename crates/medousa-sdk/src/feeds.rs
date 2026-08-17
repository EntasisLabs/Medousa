#[cfg(feature = "async")]
use medousa_types::{
    FeedLatestGoodQuery, FeedLatestGoodResponse, FeedListResponse, FeedReadRequest,
    FeedStreamEvent, FeedTailQuery, FeedTailResponse,
};

#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::Stream;
#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::StreamExt;

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::{op_path, op_path_query};
#[cfg(feature = "async")]
use crate::transport::path_with_query;

#[cfg(all(feature = "async", feature = "sse"))]
use crate::streaming::{SseLineStream, decode_sse_json};

#[cfg(feature = "async")]
pub struct FeedsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
fn feed_tail_query_params(query: &FeedTailQuery) -> Vec<(&'static str, String)> {
    let mut params = Vec::new();
    if let Some(profile_id) = &query.profile_id {
        params.push(("profile_id", profile_id.clone()));
    }
    if let Some(limit) = query.limit {
        params.push(("limit", limit.to_string()));
    }
    params
}

#[cfg(feature = "async")]
fn feed_profile_query(profile_id: Option<&str>) -> Vec<(&'static str, String)> {
    profile_id
        .map(|value| vec![("profile_id", value.to_string())])
        .unwrap_or_default()
}

fn feed_latest_good_query_params(query: &FeedLatestGoodQuery) -> Vec<(&'static str, String)> {
    feed_profile_query(query.profile_id.as_deref())
}

#[cfg(feature = "async")]
impl FeedsApi<'_> {
    pub async fn list(
        &self,
        profile_id: Option<&str>,
    ) -> Result<FeedListResponse, crate::SdkError> {
        let path = op_path_query(&ops::FEEDS_GET, &[], &feed_profile_query(profile_id))?;
        self.client.http().get(&path).await
    }

    pub async fn tail(
        &self,
        feed_id: &str,
        query: &FeedTailQuery,
    ) -> Result<FeedTailResponse, crate::SdkError> {
        let path = path_with_query(
            &op_path(
                &ops::FEEDS_BY_FEED_ID_TAIL_GET,
                &[("feed_id", feed_id.trim())],
            )?,
            &feed_tail_query_params(query),
        );
        self.client.http().get(&path).await
    }

    pub async fn latest_good(
        &self,
        feed_id: &str,
        query: &FeedLatestGoodQuery,
    ) -> Result<FeedLatestGoodResponse, crate::SdkError> {
        let path = path_with_query(
            &op_path(
                &ops::FEEDS_BY_FEED_ID_LATEST_GOOD_GET,
                &[("feed_id", feed_id.trim())],
            )?,
            &feed_latest_good_query_params(query),
        );
        self.client.http().get(&path).await
    }

    pub async fn mark_read(
        &self,
        feed_id: &str,
        request: &FeedReadRequest,
    ) -> Result<(), crate::SdkError> {
        let path = op_path(
            &ops::FEEDS_BY_FEED_ID_READ_POST,
            &[("feed_id", feed_id.trim())],
        )?;
        self.client
            .http()
            .post::<serde_json::Value, _>(&path, request)
            .await?;
        Ok(())
    }

    #[cfg(feature = "sse")]
    pub fn stream(
        &self,
        profile_id: Option<&str>,
    ) -> impl Stream<Item = Result<FeedStreamEvent, crate::SdkError>> + '_ {
        let path = op_path_query(&ops::FEEDS_STREAM_GET, &[], &feed_profile_query(profile_id))
            .expect("generated path");
        let byte_stream = self
            .client
            .transport()
            .stream_sse(self.client.base_url(), path);
        SseLineStream::new(byte_stream).map(|line| line.and_then(|data| decode_sse_json(&data)))
    }
}
