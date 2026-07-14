#[cfg(feature = "async")]
use medousa_types::{
    CalendarDeleteResponse, CalendarExportQuery, CalendarExportResponse, CalendarImportRequest,
    CalendarImportResponse, CalendarListQuery, CalendarListResponse, CalendarWriteRequest,
    CalendarWriteResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::{decode, path_with_query};

#[cfg(feature = "async")]
pub struct CalendarApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
fn list_query_params(query: &CalendarListQuery) -> Vec<(&str, String)> {
    let mut params = Vec::new();
    if let Some(from) = query.from {
        params.push(("from", from.to_rfc3339()));
    }
    if let Some(to) = query.to {
        params.push(("to", to.to_rfc3339()));
    }
    if let Some(path) = &query.path {
        params.push(("path", path.clone()));
    }
    params
}

#[cfg(feature = "async")]
impl CalendarApi<'_> {
    pub async fn list_events(
        &self,
        query: &CalendarListQuery,
    ) -> Result<CalendarListResponse, crate::SdkError> {
        let path = path_with_query("/v1/calendar/events", &list_query_params(query));
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn create_event(
        &self,
        request: &CalendarWriteRequest,
    ) -> Result<CalendarWriteResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/calendar/events", body)
            .await?;
        decode(value).await
    }

    pub async fn update_event(
        &self,
        uid: &str,
        request: &CalendarWriteRequest,
    ) -> Result<CalendarWriteResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = format!("/v1/calendar/events/{}", uid.trim());
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn delete_event(
        &self,
        uid: &str,
        query: &CalendarExportQuery,
    ) -> Result<CalendarDeleteResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(path) = &query.path {
            params.push(("path", path.clone()));
        }
        let path = path_with_query(&format!("/v1/calendar/events/{}", uid.trim()), &params);
        let value = self
            .client
            .transport()
            .delete_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn import_ics(
        &self,
        request: &CalendarImportRequest,
    ) -> Result<CalendarImportResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/calendar/import", body)
            .await?;
        decode(value).await
    }

    pub async fn export(
        &self,
        query: &CalendarExportQuery,
    ) -> Result<CalendarExportResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(path) = &query.path {
            params.push(("path", path.clone()));
        }
        let path = path_with_query("/v1/calendar/export", &params);
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }
}
