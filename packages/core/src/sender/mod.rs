use std::sync::Arc;
use tokio::sync::Mutex;

pub mod airplay;
pub mod airplay_mdns;
pub mod bluetooth;
pub mod sonos;

/// Failure of a transport sender. Variants carry a human message but callers
/// branch on the variant, never on the text.
#[derive(Debug, thiserror::Error)]
pub enum SenderError {
    /// A volume change or similar request arrived while nothing is playing.
    #[error("no active output")]
    NoActiveOutput,
    /// The requested speaker or OS audio endpoint does not exist.
    #[error("{0}")]
    DeviceNotFound(String),
    /// The sender or device cannot accept the request in its current state
    /// (not paired, helper not running, already started).
    #[error("{0}")]
    NotReady(String),
    /// The receiver, sidecar or helper process could not be reached.
    #[error("{0}")]
    Transport(String),
    /// The previously live output refused to stop, so exclusivity could not
    /// be handed over.
    #[error("could not stop the active output before switching: {0}")]
    StopFailed(Box<SenderError>),
    /// Starting failed and the attempt to restore the previous state also
    /// failed; `rollback` describes what is left behind.
    #[error("{error}; {rollback}")]
    RollbackFailed {
        error: Box<SenderError>,
        rollback: String,
    },
    /// Local failure (OS audio, thread or task error).
    #[error("{0}")]
    Internal(String),
}

impl SenderError {
    pub fn transport(message: impl Into<String>) -> Self {
        SenderError::Transport(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        SenderError::Internal(message.into())
    }

    pub fn not_ready(message: impl Into<String>) -> Self {
        SenderError::NotReady(message.into())
    }

    /// The innermost error that explains a rollback or stop failure.
    pub fn root(&self) -> &SenderError {
        match self {
            SenderError::StopFailed(inner) => inner.root(),
            SenderError::RollbackFailed { error, .. } => error.root(),
            other => other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputFormat {
    pub sample_rate_hz: u32,
    pub supported_hz: Vec<u32>,
}

#[async_trait::async_trait]
pub trait AudioSender: Send + Sync {
    async fn start(&mut self) -> Result<(), SenderError>;
    async fn stop(&mut self) -> Result<(), SenderError>;
    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError>;
    fn name(&self) -> &str;
    fn transport(&self) -> &'static str;
    /// Actual format of a live OS sink, discovered when playback starts.
    fn output_format(&self) -> Option<OutputFormat> {
        None
    }
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

    fn transport(&self) -> &'static str {
        "null"
    }
}
