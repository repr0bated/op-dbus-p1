use crate::error::Result;
use crate::execution_job::ExecutionJob;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait StateStore: Send + Sync {
    async fn save_job(&self, job: &ExecutionJob) -> Result<()>;
    async fn get_job(&self, id: Uuid) -> Result<Option<ExecutionJob>>;
    async fn update_job(&self, job: &ExecutionJob) -> Result<()>;
}
