use crate::error::Result;
use crate::execution_job::ExecutionJob;
use crate::state_store::StateStore;
use async_trait::async_trait;
use uuid::Uuid;

pub struct SqliteStore {
    // Stub
}

impl SqliteStore {
    pub async fn new(_url: &str) -> Result<Self> {
        Ok(Self {})
    }
}

#[async_trait]
impl StateStore for SqliteStore {
    async fn save_job(&self, _job: &ExecutionJob) -> Result<()> {
        Ok(())
    }

    async fn get_job(&self, _id: Uuid) -> Result<Option<ExecutionJob>> {
        Ok(None)
    }

    async fn update_job(&self, _job: &ExecutionJob) -> Result<()> {
        Ok(())
    }
}
