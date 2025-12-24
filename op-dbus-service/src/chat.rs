use zbus::interface;
use op_chat::ChatActorHandle;

pub struct ChatInterface {
    handle: ChatActorHandle,
}

impl ChatInterface {
    pub fn new(handle: ChatActorHandle) -> Self {
        Self { handle }
    }
}

#[interface(name = "org.op_dbus.Chat")]
impl ChatInterface {
    /// Send a chat message
    async fn chat(&self, message: String, session_id: String) -> zbus::fdo::Result<String> {
        let response = self.handle.chat(Some(session_id), &message).await;
        
        if response.success {
            // If result is string, return it, else JSON stringify
            if let Some(val) = response.result {
                if let Some(s) = val.as_str() {
                    Ok(s.to_string())
                } else {
                    Ok(val.to_string())
                }
            } else {
                Ok("".to_string())
            }
        } else {
            Err(zbus::fdo::Error::Failed(response.error.unwrap_or_default()))
        }
    }

/*
    /// Execute a tool directly
    async fn execute_tool(&self, name: String, arguments: String) -> zbus::fdo::Result<String> {
        let arguments: serde_json::Value = serde_json::from_str(&arguments)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;

        // Create tool request
        let request = op_core::ToolRequest {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            arguments,
            timeout_ms: Some(30000),
            metadata: Default::default(),
        };

        let (tx, rx) = oneshot::channel();

        self.handle
            .send(RpcRequest::ToolExecution {
                request,
                response_tx: tx,
            })
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

        let response = rx.await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

        Ok(serde_json::to_string(&response)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?)
    }
*/

    /// List available tools
    async fn list_tools(&self) -> zbus::fdo::Result<String> {
        let response = self.handle.list_tools().await;
        if response.success {
             if let Some(val) = response.result {
                Ok(val.to_string())
            } else {
                Ok("{}".to_string())
            }
        } else {
            Err(zbus::fdo::Error::Failed(response.error.unwrap_or_default()))
        }
    }
}
