use crate::error::Result;
use crate::execution_job::ExecutionJob;

pub struct RedisStream {
    // Stub
}

impl RedisStream {
    pub async fn new(_url: &str) -> Result<Self> {
        Ok(Self {})
    }

    pub async fn publish_job(&self, _job: &ExecutionJob) -> Result<()> {
        Ok(())
    }
}
