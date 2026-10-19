//! Provider-neutral hosted reviews. The connected workshop owns package execution
//! and credentials; SDK callers never invoke hosting CLIs on their own machine.
use crate::client::MedousaClient;
use crate::generated::ops;
use medousa_types::{
    ReviewProviderAuthRequest, ReviewProviderAuthResponse, ReviewProviderListResponse,
    ReviewProviderMutation, ReviewProviderQuery, ReviewProviderResponse,
};

pub struct ReviewsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}
impl ReviewsApi<'_> {
    pub async fn providers(&self) -> Result<ReviewProviderListResponse, crate::SdkError> {
        self.client
            .http()
            .get(ops::REVIEWS_PROVIDERS_GET.path)
            .await
    }
    pub async fn authenticate(
        &self,
        request: &ReviewProviderAuthRequest,
    ) -> Result<ReviewProviderAuthResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::REVIEWS_AUTH_POST.path, request)
            .await
    }
    pub async fn mutate(
        &self,
        request: &ReviewProviderMutation,
    ) -> Result<ReviewProviderResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::REVIEWS_MUTATE_POST.path, request)
            .await
    }
    pub async fn query(
        &self,
        request: &ReviewProviderQuery,
    ) -> Result<ReviewProviderResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::REVIEWS_QUERY_POST.path, request)
            .await
    }
}
