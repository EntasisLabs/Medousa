#[cfg(feature = "async")]
use medousa_types::{
    ArchiveAskJobRequest, ArchiveAskJobResponse, AskJobCompleteActionsRequest,
    AskJobCompleteActionsResponse, EnqueueAskRequest, EnqueuePromptRequest, EnqueueReportRequest,
    EnqueueResponse, JobReportResponse, JobResultResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::op_path;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct JobsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl JobsApi<'_> {
    pub async fn enqueue_ask(
        &self,
        request: &EnqueueAskRequest,
    ) -> Result<EnqueueResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), ops::JOBS_ASK_POST.path, body)
            .await?;
        decode(value).await
    }

    pub async fn result(&self, job_id: &str) -> Result<JobResultResponse, crate::SdkError> {
        let path = op_path(
            &ops::JOBS_BY_JOB_ID_RESULT_GET,
            &[("job_id", job_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn report(&self, job_id: &str) -> Result<JobReportResponse, crate::SdkError> {
        let path = op_path(
            &ops::JOBS_BY_JOB_ID_REPORT_GET,
            &[("job_id", job_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn enqueue_report(
        &self,
        request: &EnqueueReportRequest,
    ) -> Result<EnqueueResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), ops::JOBS_REPORT_POST.path, body)
            .await?;
        decode(value).await
    }

    pub async fn enqueue_prompt(
        &self,
        request: &EnqueuePromptRequest,
    ) -> Result<EnqueueResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), ops::JOBS_PROMPT_POST.path, body)
            .await?;
        decode(value).await
    }

    pub async fn complete_actions(
        &self,
        job_id: &str,
        request: &AskJobCompleteActionsRequest,
    ) -> Result<AskJobCompleteActionsResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::JOBS_BY_JOB_ID_COMPLETE_ACTIONS_POST,
            &[("job_id", job_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn archive(
        &self,
        job_id: &str,
        request: &ArchiveAskJobRequest,
    ) -> Result<ArchiveAskJobResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::JOBS_BY_JOB_ID_ARCHIVE_POST,
            &[("job_id", job_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }
}
