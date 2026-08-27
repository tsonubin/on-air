use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct SenderError(pub String);

impl std::fmt::Display for SenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SenderError {}

#[async_trait::async_trait]
pub trait AudioSender: Send + Sync {
    async fn start(&mut self) -> Result<(), SenderError>;
    async fn stop(&mut self) -> Result<(), SenderError>;
    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError>;
    fn name(&self) -> &str;
}

/// Test/CI fake — logs every call instead of touching real hardware or network.
pub struct NullSender {
    name: String,
    log: Arc<Mutex<Vec<String>>>,
}

impl NullSender {
    pub fn new(name: impl Into<String>, log: Arc<Mutex<Vec<String>>>) -> Self {
        NullSender {
            name: name.into(),
            log,
        }
    }
}

#[async_trait::async_trait]
impl AudioSender for NullSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        self.log.lock().await.push(format!("{}:start", self.name));
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        self.log.lock().await.push(format!("{}:stop", self.name));
        Ok(())
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        self.log
            .lock()
            .await
            .push(format!("{}:volume:{}", self.name, volume));
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}
